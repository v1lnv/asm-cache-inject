//! Software prefetch wrappers for x86_64.
//!
//! Prefetch hints tell the CPU to start loading a cache line from main memory
//! *before* the program actually reads/writes it. When used correctly inside
//! a copy loop this can hide memory latency by overlapping data fetch with
//! computation.
//!
//! On modern CPUs the hardware prefetcher is already very good at detecting
//! sequential access patterns, so these hints are most useful for:
//!   - Strided / semi-random access where hardware prefetch struggles.
//!   - Prefetching the *next* block in a sequential copy loop so that the
//!     data is already in L1 when the next iteration starts.

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::{_mm_prefetch, _MM_HINT_ET0, _MM_HINT_NTA, _MM_HINT_T0};

/// Prefetch a cache line for **reading** into L1 cache (`PREFETCHT0`).
///
/// Use this when you know data at `addr` will be *read* in the near future.
/// The CPU will start fetching the 64-byte cache line containing `addr`
/// into all cache levels (L1 / L2 / L3).
#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub fn prefetch_read(addr: *const u8) {
    // SAFETY: Prefetch is a performance hint with no architectural
    // side-effects. A bad address simply results in a NOP.
    unsafe {
        _mm_prefetch(addr as *const i8, _MM_HINT_T0);
    }
}

/// Prefetch a cache line with **write intent** into L1 cache (`PREFETCHW` /
/// `PREFETCHT0` with ET0 hint).
///
/// Equivalent to the `PREFETCHW` instruction on CPUs that support it.
/// The CPU fetches the cache line in *exclusive* state so that the
/// subsequent store does not need to perform a cache-line ownership
/// transfer (RFO), reducing store latency.
///
/// Use this before writing to a destination address in a copy loop.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub fn prefetch_write(addr: *const u8) {
    // SAFETY: Same as `prefetch_read` — purely advisory, no side-effects.
    unsafe {
        _mm_prefetch(addr as *const i8, _MM_HINT_ET0);
    }
}

/// Prefetch a cache line as **non-temporal** (`PREFETCHNTA`).
///
/// Hints the CPU that the data will be accessed only once and should not
/// pollute the L1/L2 caches. The line may be placed in a streaming buffer
/// or in L3 only (implementation dependent).
///
/// Use this when reading large buffers that will not be re-used.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub fn prefetch_nta(addr: *const u8) {
    // SAFETY: Same as above — purely advisory.
    unsafe {
        _mm_prefetch(addr as *const i8, _MM_HINT_NTA);
    }
}

// Fallback stubs for non-x86_64 targets

#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
pub fn prefetch_read(_addr: *const u8) {
    // No-op on non-x86_64 targets.
}

#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
pub fn prefetch_write(_addr: *const u8) {
    // No-op on non-x86_64 targets.
}

#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
pub fn prefetch_nta(_addr: *const u8) {
    // No-op on non-x86_64 targets.
}
