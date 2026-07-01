//! Thin wrappers around x86_64 memory fence instructions.
//!
//! Non-temporal stores (`MOVNTDQ`) are *weakly ordered* — the CPU is allowed
//! to reorder them relative to other stores. A store fence (`SFENCE`) forces
//! all preceding stores (including non-temporal ones) to become globally
//! visible before any subsequent store executes.
//!
//! These wrappers exist so that the rest of the codebase never needs to
//! import arch-specific intrinsics directly.

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::{_mm_mfence, _mm_sfence};

/// Issue a **store fence** (`SFENCE`).
///
/// Guarantees that all preceding store instructions (including non-temporal
/// streaming stores) are globally visible before any subsequent store.
///
/// Must be called after a batch of `MOVNTDQ` / `_mm_stream_si128` writes
/// to ensure the data has been flushed from the CPU's write-combining
/// buffers to main memory.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub fn store_fence() {
    // SAFETY: `SFENCE` is a serialising instruction with no side-effects
    // beyond memory ordering. It is always safe to execute on x86_64.
    unsafe {
        _mm_sfence();
    }
}

/// Issue a **full memory fence** (`MFENCE`).
///
/// Guarantees that all preceding loads *and* stores are globally visible
/// before any subsequent memory operation. Stronger than [`store_fence`]
/// but also more expensive — use only when load ordering matters.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub fn full_fence() {
    // SAFETY: `MFENCE` is always safe to execute on x86_64.
    unsafe {
        _mm_mfence();
    }
}

// Fallback stubs for non-x86_64 targets (allows `cargo check` on ARM, etc.)

#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
pub fn store_fence() {
    std::sync::atomic::fence(std::sync::atomic::Ordering::Release);
}

#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
pub fn full_fence() {
    std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst);
}
