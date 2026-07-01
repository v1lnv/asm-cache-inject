//! Non-temporal (streaming) memory copy using SSE2 `MOVNTDQ` instructions.
//!
//! Standard `memcpy` loads data into the CPU cache hierarchy (L1→L2→L3).
//! When copying large buffers (hundreds of MiB) this "pollutes" the cache
//! with data that will never be re-read by the CPU, evicting useful hot data.
//!
//! Non-temporal stores bypass the cache entirely — data is written directly
//! to main memory through the CPU's write-combining buffers. This keeps the
//! cache clean for other workloads and can be faster for pure streaming writes
//! because it avoids the Read-For-Ownership (RFO) penalty.
//!
//! The inner loop processes **64 bytes per iteration** (one full cache line)
//! using four `_mm_stream_si128` calls (4 × 16 B = 64 B). A `PREFETCHW` hint
//! on the destination and a `PREFETCHT0` hint on the source are issued one
//! cache line ahead to hide memory latency.

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::{__m128i, _mm_load_si128, _mm_stream_si128};

use super::fence::store_fence;
use super::prefetch::{prefetch_read, prefetch_write};

/// Size of one x86_64 cache line in bytes.
const CACHE_LINE_SIZE: usize = 64;

/// Number of bytes processed per inner-loop iteration (must equal
/// `CACHE_LINE_SIZE` for optimal write-combining).
const CHUNK_SIZE: usize = CACHE_LINE_SIZE;

/// Copy `len` bytes from `src` to `dst` using **non-temporal streaming stores**
/// (`MOVNTDQ`), bypassing the CPU cache hierarchy.
///
/// # Constraints
///
/// - `dst` and `src` must both be **16-byte aligned** (guaranteed by the
///   page-aligned pool allocator).
/// - `len` must be a **multiple of 64** (one cache line).
/// - `dst` and `src` must not overlap.
///
/// # Safety
///
/// This is an `unsafe` function because it operates on raw pointers and
/// executes SIMD instructions. The caller must uphold the alignment and
/// size invariants listed above.
///
/// # Post-conditions
///
/// A `SFENCE` is issued after the copy loop to ensure all non-temporal
/// stores are globally visible before the function returns.
#[cfg(target_arch = "x86_64")]
pub unsafe fn nt_memcpy(dst: *mut u8, src: *const u8, len: usize) {
    debug_assert_eq!(
        len % CHUNK_SIZE,
        0,
        "len must be a multiple of {CHUNK_SIZE}"
    );
    debug_assert_eq!(dst as usize % 16, 0, "dst must be 16-byte aligned");
    debug_assert_eq!(src as usize % 16, 0, "src must be 16-byte aligned");
    debug_assert!(
        !std::ptr::eq(dst, src as *mut u8),
        "src and dst must not be the same pointer"
    );

    let num_chunks = len / CHUNK_SIZE;

    for i in 0..num_chunks {
        let offset = i * CHUNK_SIZE;

        // -- Prefetch the *next* cache line while we process the current one.
        if i + 1 < num_chunks {
            let next_offset = (i + 1) * CHUNK_SIZE;
            prefetch_read(src.add(next_offset));
            prefetch_write(dst.add(next_offset));
        }

        // -- Load 64 bytes from source (4 × 128-bit SSE registers).
        let s0 = _mm_load_si128(src.add(offset) as *const __m128i);
        let s1 = _mm_load_si128(src.add(offset + 16) as *const __m128i);
        let s2 = _mm_load_si128(src.add(offset + 32) as *const __m128i);
        let s3 = _mm_load_si128(src.add(offset + 48) as *const __m128i);

        // -- Stream 64 bytes to destination (non-temporal, cache-bypassing).
        _mm_stream_si128(dst.add(offset) as *mut __m128i, s0);
        _mm_stream_si128(dst.add(offset + 16) as *mut __m128i, s1);
        _mm_stream_si128(dst.add(offset + 32) as *mut __m128i, s2);
        _mm_stream_si128(dst.add(offset + 48) as *mut __m128i, s3);
    }

    // -- Ensure all streaming stores have reached main memory.
    store_fence();
}

/// Fallback for non-x86_64 targets — delegates to `std::ptr::copy_nonoverlapping`.
#[cfg(not(target_arch = "x86_64"))]
pub unsafe fn nt_memcpy(dst: *mut u8, src: *const u8, len: usize) {
    std::ptr::copy_nonoverlapping(src, dst, len);
}

// Unit tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::aligned_allocator::AlignedBuffer;

    #[test]
    fn nt_copy_matches_standard_copy() {
        let size = 4096; // one page
        let mut src_buf = AlignedBuffer::new(size).unwrap();
        let mut dst_buf = AlignedBuffer::new(size).unwrap();

        // Fill source with a known pattern.
        for (i, byte) in src_buf.as_mut().iter_mut().enumerate() {
            *byte = (i % 256) as u8;
        }

        unsafe {
            nt_memcpy(dst_buf.as_mut_ptr(), src_buf.as_ptr(), size);
        }

        assert_eq!(src_buf.as_ref(), dst_buf.as_ref());
    }

    #[test]
    fn nt_copy_large_buffer() {
        let size = 1024 * 1024; // 1 MiB
        let mut src_buf = AlignedBuffer::new(size).unwrap();
        let mut dst_buf = AlignedBuffer::new(size).unwrap();

        for (i, byte) in src_buf.as_mut().iter_mut().enumerate() {
            *byte = ((i * 7 + 3) % 256) as u8;
        }

        unsafe {
            nt_memcpy(dst_buf.as_mut_ptr(), src_buf.as_ptr(), size);
        }

        assert_eq!(src_buf.as_ref(), dst_buf.as_ref());
    }
}
