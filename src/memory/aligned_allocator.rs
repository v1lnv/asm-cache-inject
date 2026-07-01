//! Provides page-aligned memory allocation required for O_DIRECT I/O.
//!
//! The Linux kernel's O_DIRECT flag demands that user-space buffers are aligned
//! to the block-device sector size (typically 512 B) **and** to the filesystem
//! page size (4096 B). We enforce the stricter 4096-byte alignment everywhere
//! so that every buffer is unconditionally safe for direct I/O.
//!
//! The allocator uses `std::alloc::alloc` with a custom `Layout` rather than
//! libc `posix_memalign` so that deallocation goes through the Rust global
//! allocator and is tracked by tools like Miri and Valgrind.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::NonNull;

use crate::error::{CacheError, CacheResult};

/// Default alignment for all I/O buffers — one OS page (4 KiB).
pub const PAGE_ALIGNMENT: usize = 4096;

/// A heap-allocated byte buffer whose start address is guaranteed to be
/// aligned to [`PAGE_ALIGNMENT`] (4096 bytes).
///
/// The buffer is zero-initialised on creation and automatically freed when
/// dropped. It is **not** `Clone` — ownership is unique so that we never
/// accidentally alias a DMA-visible region.
pub struct AlignedBuffer {
    /// Raw pointer to the start of the allocation.
    ptr: NonNull<u8>,
    /// Allocation layout (carries both size and alignment).
    layout: Layout,
}

// SAFETY: The buffer is a simple contiguous byte region with no interior
// mutability or thread-local state. Sending it across threads is safe as
// long as only one thread writes at a time (enforced by &mut).
unsafe impl Send for AlignedBuffer {}
unsafe impl Sync for AlignedBuffer {}

impl AlignedBuffer {
    /// Allocate a new page-aligned buffer of `size` bytes.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::Allocation`] if:
    /// - `size` is zero.
    /// - `size` is not a multiple of [`PAGE_ALIGNMENT`].
    /// - The underlying allocator returns a null pointer.
    pub fn new(size: usize) -> CacheResult<Self> {
        if size == 0 {
            return Err(CacheError::Allocation {
                reason: "requested size is zero".into(),
            });
        }
        if !size.is_multiple_of(PAGE_ALIGNMENT) {
            return Err(CacheError::Allocation {
                reason: format!("size ({size}) must be a multiple of {PAGE_ALIGNMENT}"),
            });
        }

        let layout =
            Layout::from_size_align(size, PAGE_ALIGNMENT).map_err(|e| CacheError::Allocation {
                reason: format!("invalid layout: {e}"),
            })?;

        // SAFETY: `layout` has non-zero size and a valid power-of-two alignment.
        let raw = unsafe { alloc_zeroed(layout) };

        let ptr = NonNull::new(raw).ok_or_else(|| CacheError::Allocation {
            reason: format!(
                "allocator returned null for {size} bytes @ {PAGE_ALIGNMENT}-byte alignment"
            ),
        })?;

        debug_assert_eq!(
            ptr.as_ptr() as usize % PAGE_ALIGNMENT,
            0,
            "allocator returned a mis-aligned pointer"
        );

        Ok(Self { ptr, layout })
    }

    /// Returns a raw pointer to the start of the buffer.
    ///
    /// The pointer is guaranteed to be non-null and aligned to
    /// [`PAGE_ALIGNMENT`].
    #[inline]
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    /// Returns a mutable raw pointer to the start of the buffer.
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// Total size of the allocation in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.layout.size()
    }

    /// Returns `true` if the buffer has zero length (always `false` after
    /// successful construction, but included for API completeness).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.layout.size() == 0
    }
}

impl AsRef<[u8]> for AlignedBuffer {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        // SAFETY: The pointer is valid for `layout.size()` bytes and the
        // buffer is alive for the duration of `&self`.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.layout.size()) }
    }
}

impl AsMut<[u8]> for AlignedBuffer {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        // SAFETY: Exclusive `&mut self` guarantees no aliasing.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.layout.size()) }
    }
}

impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        // SAFETY: `self.ptr` was allocated by `alloc_zeroed` with `self.layout`.
        unsafe {
            dealloc(self.ptr.as_ptr(), self.layout);
        }
    }
}

// Unit tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_aligned_buffer() {
        let buf = AlignedBuffer::new(PAGE_ALIGNMENT).expect("allocation failed");
        assert_eq!(buf.len(), PAGE_ALIGNMENT);
        assert_eq!(buf.as_ptr() as usize % PAGE_ALIGNMENT, 0);
    }

    #[test]
    fn buffer_is_zero_initialised() {
        let buf = AlignedBuffer::new(PAGE_ALIGNMENT).expect("allocation failed");
        assert!(buf.as_ref().iter().all(|&b| b == 0));
    }

    #[test]
    fn rejects_zero_size() {
        assert!(AlignedBuffer::new(0).is_err());
    }

    #[test]
    fn rejects_misaligned_size() {
        assert!(AlignedBuffer::new(1000).is_err());
    }

    #[test]
    fn large_allocation() {
        // 1 MiB — should succeed on any modern system.
        let buf = AlignedBuffer::new(1024 * 1024).expect("allocation failed");
        assert_eq!(buf.len(), 1024 * 1024);
        assert_eq!(buf.as_ptr() as usize % PAGE_ALIGNMENT, 0);
    }
}
