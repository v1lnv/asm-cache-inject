//! Sequential read/write benchmarks.
//!
//! Sequential access measures sustained throughput — the most important metric
//! for large file transfers on block devices where data is read or written
//! sequentially across contiguous sectors.

use std::time::{Duration, Instant};

use crate::engine::CacheEngine;
use crate::error::CacheResult;
use crate::io::{read_lba, write_lba};
use crate::memory::AlignedBuffer;

/// Result of a sequential benchmark run.
#[derive(Debug, Clone)]
pub struct SequentialResult {
    /// Type of benchmark (read or write).
    pub operation: String,
    /// Whether the cache was active during this run.
    pub cached: bool,
    /// Number of blocks processed.
    pub block_count: usize,
    /// Size of each block in bytes.
    pub block_size: usize,
    /// Total elapsed wall-clock time.
    pub elapsed: Duration,
    /// Number of cache hits (only meaningful when `cached == true`).
    pub cache_hits: u64,
    /// Number of cache misses.
    pub cache_misses: u64,
}

impl SequentialResult {
    /// Total bytes transferred.
    pub fn total_bytes(&self) -> usize {
        self.block_count * self.block_size
    }

    /// Throughput in megabytes per second.
    pub fn throughput_mbps(&self) -> f64 {
        let bytes = self.total_bytes() as f64;
        let secs = self.elapsed.as_secs_f64();
        if secs == 0.0 {
            return 0.0;
        }
        bytes / (1024.0 * 1024.0) / secs
    }

    /// I/O operations per second.
    pub fn iops(&self) -> f64 {
        let secs = self.elapsed.as_secs_f64();
        if secs == 0.0 {
            return 0.0;
        }
        self.block_count as f64 / secs
    }

    /// Average latency per block in microseconds.
    pub fn avg_latency_us(&self) -> f64 {
        if self.block_count == 0 {
            return 0.0;
        }
        self.elapsed.as_micros() as f64 / self.block_count as f64
    }
}

/// Run a sequential **read** benchmark directly against the device
/// (no cache). Reads `block_count` consecutive blocks starting at LBA 0.
pub fn bench_sequential_read_direct(
    fd: i32,
    block_size: usize,
    block_count: usize,
) -> CacheResult<SequentialResult> {
    let mut buf = AlignedBuffer::new(block_size)?;

    let start = Instant::now();
    for lba in 0..block_count as u64 {
        read_lba(fd, lba, block_size, buf.as_mut())?;
    }
    let elapsed = start.elapsed();

    Ok(SequentialResult {
        operation: "Sequential Read".into(),
        cached: false,
        block_count,
        block_size,
        elapsed,
        cache_hits: 0,
        cache_misses: block_count as u64,
    })
}

/// Run a sequential **read** benchmark through the cache engine.
pub fn bench_sequential_read_cached(
    engine: &CacheEngine,
    block_count: usize,
) -> CacheResult<SequentialResult> {
    let block_size = engine.block_size();
    let mut buf = AlignedBuffer::new(block_size)?;
    let mut hits: u64 = 0;
    let mut misses: u64 = 0;

    let start = Instant::now();
    for lba in 0..block_count as u64 {
        let result = engine.read(lba, buf.as_mut())?;
        if result.cache_hit {
            hits += 1;
        } else {
            misses += 1;
        }
    }
    let elapsed = start.elapsed();

    Ok(SequentialResult {
        operation: "Sequential Read".into(),
        cached: true,
        block_count,
        block_size,
        elapsed,
        cache_hits: hits,
        cache_misses: misses,
    })
}

/// Run a sequential **write** benchmark directly against the device.
pub fn bench_sequential_write_direct(
    fd: i32,
    block_size: usize,
    block_count: usize,
) -> CacheResult<SequentialResult> {
    let mut buf = AlignedBuffer::new(block_size)?;
    // Fill buffer with a recognizable pattern.
    for (i, byte) in buf.as_mut().iter_mut().enumerate() {
        *byte = (i % 256) as u8;
    }

    let start = Instant::now();
    for lba in 0..block_count as u64 {
        write_lba(fd, lba, block_size, buf.as_ref())?;
    }
    let elapsed = start.elapsed();

    Ok(SequentialResult {
        operation: "Sequential Write".into(),
        cached: false,
        block_count,
        block_size,
        elapsed,
        cache_hits: 0,
        cache_misses: 0,
    })
}

/// Run a sequential **write** benchmark through the cache engine.
pub fn bench_sequential_write_cached(
    engine: &CacheEngine,
    block_count: usize,
) -> CacheResult<SequentialResult> {
    let block_size = engine.block_size();
    let mut buf = AlignedBuffer::new(block_size)?;
    for (i, byte) in buf.as_mut().iter_mut().enumerate() {
        *byte = (i % 256) as u8;
    }

    let mut updates: u64 = 0;
    let mut inserts: u64 = 0;

    let start = Instant::now();
    for lba in 0..block_count as u64 {
        let result = engine.write(lba, buf.as_ref())?;
        if result.was_update {
            updates += 1;
        } else {
            inserts += 1;
        }
    }
    let elapsed = start.elapsed();

    Ok(SequentialResult {
        operation: "Sequential Write".into(),
        cached: true,
        block_count,
        block_size,
        elapsed,
        cache_hits: updates,
        cache_misses: inserts,
    })
}
