//! A thin wrapper around a raw pointer into the memory pool. It represents a
//! single cache block (typically 4096 bytes) and provides safe slice access
//! without owning the underlying memory — the pool retains ownership.
//!
//! This type is used throughout the engine to pass around references to
//! individual cached blocks without copying data.

use std::fmt;

/// A borrowed view into one block inside the memory pool.
///
/// `BlockBuffer` does **not** own its memory — the [`super::pool::MemoryPool`]
/// does. Dropping a `BlockBuffer` does not free the block; the block must be
/// explicitly returned to the pool via its free-list.
pub struct BlockBuffer {
    /// Pointer to the first byte of the block within the pool region.
    ptr: *mut u8,
    /// Size of this block in bytes (always == pool block size).
    size: usize,
}

// SAFETY: The pool guarantees that each block is handed out to at most one
// owner at a time, so sending across threads is safe.
unsafe impl Send for BlockBuffer {}
unsafe impl Sync for BlockBuffer {}

impl BlockBuffer {
    /// Create a new `BlockBuffer` from a raw pointer and size.
    ///
    /// # Safety
    ///
    /// - `ptr` must point to a valid, page-aligned allocation of at least
    ///   `size` bytes that will remain valid for the lifetime of this handle.
    /// - The caller must ensure exclusive access to the pointed-to region
    ///   for the duration of any `&mut` borrow.
    pub(crate) unsafe fn from_raw(ptr: *mut u8, size: usize) -> Self {
        debug_assert!(!ptr.is_null(), "BlockBuffer created from null pointer");
        debug_assert!(
            (ptr as usize).is_multiple_of(super::aligned_allocator::PAGE_ALIGNMENT),
            "BlockBuffer pointer is not page-aligned"
        );
        Self { ptr, size }
    }

    /// Returns a raw pointer to the start of the block.
    #[inline]
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    /// Returns a mutable raw pointer to the start of the block.
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }

    /// Block size in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.size
    }

    /// Always `false` for a validly constructed block buffer.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Zero-fill the entire block.
    pub fn clear(&mut self) {
        // SAFETY: `self.ptr` is valid for `self.size` bytes.
        unsafe {
            std::ptr::write_bytes(self.ptr, 0, self.size);
        }
    }
}

impl AsRef<[u8]> for BlockBuffer {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        // SAFETY: Pointer is valid for `self.size` bytes; borrow is tied to
        // the lifetime of `&self`.
        unsafe { std::slice::from_raw_parts(self.ptr, self.size) }
    }
}

impl AsMut<[u8]> for BlockBuffer {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        // SAFETY: `&mut self` guarantees exclusive access.
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.size) }
    }
}

impl fmt::Debug for BlockBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockBuffer")
            .field("ptr", &self.ptr)
            .field("size", &self.size)
            .finish()
    }
}
