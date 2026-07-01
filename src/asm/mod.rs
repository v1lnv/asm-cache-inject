//! Module facade for the x86_64 inline assembly routines.
//!
//! Provides a unified `fast_copy` entry point that auto-selects the optimal
//! copy strategy based on runtime CPU feature detection:
//!
//!   1. **Non-temporal** (`MOVNTDQ`) — bypasses CPU cache; best for large
//!      streaming writes where data will not be re-read by the CPU.
//!   2. **ERMS** (`REP MOVSB`) — temporal copy that benefits from CPU cache;
//!      best for read-hits where data should stay warm.
//!   3. **Fallback** — `std::ptr::copy_nonoverlapping` for non-x86_64 targets.
//!
//! The strategy is selected once at startup and stored in a function pointer
//! so there is no per-call branch overhead.

pub mod fast_memcpy;
pub mod fence;
pub mod nontemporal;
pub mod prefetch;

// Re-export the primary public symbols.
pub use fast_memcpy::{cpu_supports_erms, erms_memcpy};
pub use fence::{full_fence, store_fence};
pub use nontemporal::nt_memcpy;
pub use prefetch::{prefetch_nta, prefetch_read, prefetch_write};

/// Copy strategy selector — determined once at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyStrategy {
    /// Non-temporal streaming stores (`MOVNTDQ`). Bypasses CPU cache.
    NonTemporal,
    /// Enhanced REP MOVSB. Temporal (cache-friendly).
    Erms,
    /// Plain `memcpy` fallback.
    Generic,
}

impl std::fmt::Display for CopyStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonTemporal => write!(f, "MOVNTDQ (non-temporal streaming)"),
            Self::Erms => write!(f, "REP MOVSB (Enhanced REP MOVSB)"),
            Self::Generic => write!(f, "generic memcpy (fallback)"),
        }
    }
}

/// Detect the best available copy strategy for the current CPU.
///
/// Called once during engine initialisation. The result should be cached
/// and passed to [`fast_copy`] / [`fast_copy_nontemporal`] as needed.
pub fn detect_strategy() -> CopyStrategy {
    #[cfg(target_arch = "x86_64")]
    {
        // SSE2 is mandatory on x86_64, so MOVNTDQ is always available.
        // ERMS is optional (Ivy Bridge+).
        if cpu_supports_erms() {
            log::info!("CPU supports Enhanced REP MOVSB (ERMS)");
        } else {
            log::info!("CPU does not support ERMS; using MOVNTDQ for streaming");
        }
        // Non-temporal is preferred for large I/O; ERMS is used for
        // cache-hit reads. Both are always available on x86_64 (SSE2).
        CopyStrategy::NonTemporal
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        log::warn!("Non-x86_64 target: using generic memcpy fallback");
        CopyStrategy::Generic
    }
}

/// Copy `len` bytes from `src` to `dst` using the **non-temporal** path
/// (bypasses CPU cache). Use for writes from user buffer → cache pool and
/// for flush operations from pool → device buffer.
///
/// # Safety
///
/// Same invariants as [`nt_memcpy`]: both pointers must be 16-byte aligned,
/// `len` must be a multiple of 64, and the regions must not overlap.
#[inline]
pub unsafe fn fast_copy_nontemporal(dst: *mut u8, src: *const u8, len: usize) {
    nt_memcpy(dst, src, len);
}

/// Copy `len` bytes from `src` to `dst` using the **temporal** (cache-warm)
/// path. Use for cache-hit reads (pool → user buffer) where keeping data
/// in CPU cache benefits subsequent accesses.
///
/// Falls back to ERMS if available, otherwise uses generic copy.
///
/// # Safety
///
/// - Both pointers must be valid for the given length.
/// - Regions must not overlap.
#[inline]
pub unsafe fn fast_copy_temporal(dst: *mut u8, src: *const u8, len: usize) {
    if cpu_supports_erms() {
        erms_memcpy(dst, src, len);
    } else {
        std::ptr::copy_nonoverlapping(src, dst, len);
    }
}

/// Log a summary of detected CPU capabilities relevant to the ASM layer.
pub fn log_cpu_capabilities() {
    let strategy = detect_strategy();
    log::info!("Primary copy strategy: {strategy}");

    #[cfg(target_arch = "x86_64")]
    {
        log::info!("SSE2: always available on x86_64");
        log::info!(
            "ERMS: {}",
            if cpu_supports_erms() {
                "supported"
            } else {
                "not supported"
            }
        );
    }
}
