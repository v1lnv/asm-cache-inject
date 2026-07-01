//! Flush scheduling configuration.
//!
//! The scheduler controls two aspects of write-back timing:
//!
//!   1. **Periodic interval** — the flush thread wakes up every N seconds
//!      regardless of whether any blocks are dirty.
//!   2. **Dirty watermark** — if the fraction of dirty blocks exceeds a
//!      threshold, the engine sends an immediate wake signal to the flush
//!      thread instead of waiting for the next interval.

use std::time::Duration;

/// Configuration for the background flush scheduler.
#[derive(Debug, Clone)]
pub struct FlushSchedule {
    /// Time between periodic flush cycles.
    pub interval: Duration,

    /// Fraction of the cache that may be dirty before an **immediate** flush
    /// is triggered (0.0 = flush on every write, 1.0 = only periodic).
    ///
    /// A reasonable default is 0.75 — when 75 % of cached blocks are dirty,
    /// start flushing immediately to avoid running out of clean eviction
    /// candidates.
    pub dirty_watermark: f64,
}

impl FlushSchedule {
    /// Create a new schedule with the given interval (in seconds) and
    /// dirty watermark (0.0–1.0).
    ///
    /// # Panics
    ///
    /// Panics if `dirty_watermark` is not in `[0.0, 1.0]`.
    pub fn new(interval_secs: u64, dirty_watermark: f64) -> Self {
        assert!(
            (0.0..=1.0).contains(&dirty_watermark),
            "dirty_watermark must be in [0.0, 1.0], got {dirty_watermark}"
        );
        Self {
            interval: Duration::from_secs(interval_secs),
            dirty_watermark,
        }
    }

    /// Check whether the current dirty ratio exceeds the watermark.
    ///
    /// # Arguments
    ///
    /// - `dirty_count` — number of dirty blocks in the cache.
    /// - `total_count` — total number of occupied cache slots.
    pub fn should_flush_immediately(&self, dirty_count: usize, total_count: usize) -> bool {
        if total_count == 0 {
            return false;
        }
        let ratio = dirty_count as f64 / total_count as f64;
        ratio >= self.dirty_watermark
    }
}

impl Default for FlushSchedule {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            dirty_watermark: 0.75,
        }
    }
}
