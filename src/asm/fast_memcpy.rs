//! High-speed memory copy using `REP MOVSB` (Enhanced REP MOVSB — ERMS).
//!
//! Modern x86_64 CPUs (Ivy Bridge and later) implement ERMS which makes the
//! `REP MOVSB` instruction competitive with, or faster than, hand-tuned SSE
//! copy loops for large aligned transfers. The CPU microcode uses the widest
//! available data path internally and handles alignment / cache-line splits
//! transparently.
//!
//! This routine is used as the **cache-temporal** copy path — when we *want*
//! the data to remain in CPU cache (e.g. copying from pool → user buffer for
//! a cache-hit read). For cache-*bypassing* copies, see `nontemporal.rs`.
//!
//! A compile-time feature check is performed via `is_x86_feature_detected!`
//! at the call-site to decide whether to use this or a generic fallback.

/// Copy `len` bytes from `src` to `dst` using the `REP MOVSB` instruction.
///
/// On CPUs with Enhanced REP MOVSB (ERMS) this can match or exceed the
/// throughput of hand-written SIMD loops because the CPU microcode uses
/// the widest internal data path and handles alignment automatically.
///
/// # Safety
///
/// - `src` must be valid for reads of `len` bytes.
/// - `dst` must be valid for writes of `len` bytes.
/// - `src` and `dst` must not overlap.
/// - Both pointers should ideally be page-aligned for best throughput.
#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn erms_memcpy(dst: *mut u8, src: *const u8, len: usize) {
    debug_assert!(
        !std::ptr::eq(dst, src as *mut u8),
        "src and dst must not be the same pointer"
    );

    // `REP MOVSB` copies ECX/RCX bytes from DS:[RSI] → ES:[RDI].
    // The Rust inline assembler handles register allocation for us.
    core::arch::asm!(
        "rep movsb",
        inout("rcx") len => _,     // RCX = byte count (consumed to 0)
        inout("rdi") dst => _,     // RDI = destination pointer (advanced)
        inout("rsi") src => _,     // RSI = source pointer (advanced)
        options(nostack, preserves_flags)
    );
}

/// Fallback for non-x86_64 targets.
#[cfg(not(target_arch = "x86_64"))]
#[inline]
pub unsafe fn erms_memcpy(dst: *mut u8, src: *const u8, len: usize) {
    std::ptr::copy_nonoverlapping(src, dst, len);
}

/// Detect at runtime whether the current CPU supports Enhanced REP MOVSB.
///
/// Checks CPUID leaf 7 (structured extended features), ECX bit 9 (ERMS).
/// The result is cached internally by `is_x86_feature_detected!` on first
/// call so subsequent calls are essentially free.
#[cfg(target_arch = "x86_64")]
pub fn cpu_supports_erms() -> bool {
    is_x86_feature_detected!("ermsb")
}

#[cfg(not(target_arch = "x86_64"))]
pub fn cpu_supports_erms() -> bool {
    false
}

// Unit tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::aligned_allocator::AlignedBuffer;

    #[test]
    fn erms_copy_matches_standard_copy() {
        let size = 4096;
        let mut src_buf = AlignedBuffer::new(size).unwrap();
        let mut dst_buf = AlignedBuffer::new(size).unwrap();

        for (i, byte) in src_buf.as_mut().iter_mut().enumerate() {
            *byte = (i % 256) as u8;
        }

        unsafe {
            erms_memcpy(dst_buf.as_mut_ptr(), src_buf.as_ptr(), size);
        }

        assert_eq!(src_buf.as_ref(), dst_buf.as_ref());
    }

    #[test]
    fn erms_copy_large_buffer() {
        let size = 2 * 1024 * 1024; // 2 MiB
        let mut src_buf = AlignedBuffer::new(size).unwrap();
        let mut dst_buf = AlignedBuffer::new(size).unwrap();

        for (i, byte) in src_buf.as_mut().iter_mut().enumerate() {
            *byte = ((i * 13 + 7) % 256) as u8;
        }

        unsafe {
            erms_memcpy(dst_buf.as_mut_ptr(), src_buf.as_ptr(), size);
        }

        assert_eq!(src_buf.as_ref(), dst_buf.as_ref());
    }

    #[test]
    fn erms_detection_does_not_panic() {
        // Just ensure the detection function runs without crashing.
        let _supports = cpu_supports_erms();
    }
}
