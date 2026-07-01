//! Random-access read/write benchmarks.
//!
//! Random I/O is the worst-case scenario for physical storage drives (especially
//! mechanical HDDs) because operations require seeking/translation. This is where
//! the RAM cache shines —
//! repeated accesses to the same LBAs are served entirely from memory.

use std::time::{Duration, Instant};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::engine::CacheEngine;
use crate::error::CacheResult;
use crate::io::{read_lba, write_lba};
use crate::memory::AlignedBuffer;

/// Result of a random-access benchmark run.
#[derive(Debug, Clone)]
pub struct RandomResult {
    /// Type of benchmark (read or write).
    pub operation: String,
    /// Whether the cache was active.
    pub cached: bool,
    /// Number of I/O operations performed.
    pub op_count: usize,
    /// Size of each block in bytes.
    pub block_size: usize,
    /// Range of LBAs used (0..max_lba).
    pub lba_range: u64,
    /// Total elapsed time.
    pub elapsed: Duration,
    /// Cache hits (only when cached).
    pub cache_hits: u64,
    /// Cache misses.
    pub cache_misses: u64,
}

impl RandomResult {
    /// Total bytes transferred.
    pub fn total_bytes(&self) -> usize {
        self.op_count * self.block_size
    }

    /// Throughput in MB/s.
    pub fn throughput_mbps(&self) -> f64 {
        let bytes = self.total_bytes() as f64;
        let secs = self.elapsed.as_secs_f64();
        if secs == 0.0 {
            return 0.0;
        }
        bytes / (1024.0 * 1024.0) / secs
    }

    /// IOPS.
    pub fn iops(&self) -> f64 {
        let secs = self.elapsed.as_secs_f64();
        if secs == 0.0 {
            return 0.0;
        }
        self.op_count as f64 / secs
    }

    /// Average latency per operation in microseconds.
    pub fn avg_latency_us(&self) -> f64 {
        if self.op_count == 0 {
            return 0.0;
        }
        self.elapsed.as_micros() as f64 / self.op_count as f64
    }
}

/// Generate a deterministic sequence of random LBAs for reproducible benchmarks.
fn generate_random_lbas(count: usize, max_lba: u64, seed: u64) -> Vec<u64> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..count).map(|_| rng.gen_range(0..max_lba)).collect()
}

/// Default seed for reproducibility across runs.
const BENCH_SEED: u64 = 0xDEAD_BEEF_CAFE_F00D;

/// Run a random **read** benchmark directly against the device.
pub fn bench_random_read_direct(
    fd: i32,
    block_size: usize,
    op_count: usize,
    max_lba: u64,
) -> CacheResult<RandomResult> {
    let lbas = generate_random_lbas(op_count, max_lba, BENCH_SEED);
    let mut buf = AlignedBuffer::new(block_size)?;

    let start = Instant::now();
    for &lba in &lbas {
        read_lba(fd, lba, block_size, buf.as_mut())?;
    }
    let elapsed = start.elapsed();

    Ok(RandomResult {
        operation: "Random Read".into(),
        cached: false,
        op_count,
        block_size,
        lba_range: max_lba,
        elapsed,
        cache_hits: 0,
        cache_misses: op_count as u64,
    })
}

/// Run a random **read** benchmark through the cache engine.
pub fn bench_random_read_cached(
    engine: &CacheEngine,
    op_count: usize,
    max_lba: u64,
) -> CacheResult<RandomResult> {
    let block_size = engine.block_size();
    let lbas = generate_random_lbas(op_count, max_lba, BENCH_SEED);
    let mut buf = AlignedBuffer::new(block_size)?;
    let mut hits: u64 = 0;
    let mut misses: u64 = 0;

    let start = Instant::now();
    for &lba in &lbas {
        let result = engine.read(lba, buf.as_mut())?;
        if result.cache_hit {
            hits += 1;
        } else {
            misses += 1;
        }
    }
    let elapsed = start.elapsed();

    Ok(RandomResult {
        operation: "Random Read".into(),
        cached: true,
        op_count,
        block_size,
        lba_range: max_lba,
        elapsed,
        cache_hits: hits,
        cache_misses: misses,
    })
}

/// Run a random **write** benchmark directly against the device.
pub fn bench_random_write_direct(
    fd: i32,
    block_size: usize,
    op_count: usize,
    max_lba: u64,
) -> CacheResult<RandomResult> {
    let lbas = generate_random_lbas(op_count, max_lba, BENCH_SEED);
    let mut buf = AlignedBuffer::new(block_size)?;
    for (i, byte) in buf.as_mut().iter_mut().enumerate() {
        *byte = (i % 256) as u8;
    }

    let start = Instant::now();
    for &lba in &lbas {
        write_lba(fd, lba, block_size, buf.as_ref())?;
    }
    let elapsed = start.elapsed();

    Ok(RandomResult {
        operation: "Random Write".into(),
        cached: false,
        op_count,
        block_size,
        lba_range: max_lba,
        elapsed,
        cache_hits: 0,
        cache_misses: 0,
    })
}

/// Run a random **write** benchmark through the cache engine.
pub fn bench_random_write_cached(
    engine: &CacheEngine,
    op_count: usize,
    max_lba: u64,
) -> CacheResult<RandomResult> {
    let block_size = engine.block_size();
    let lbas = generate_random_lbas(op_count, max_lba, BENCH_SEED);
    let mut buf = AlignedBuffer::new(block_size)?;
    for (i, byte) in buf.as_mut().iter_mut().enumerate() {
        *byte = (i % 256) as u8;
    }

    let mut updates: u64 = 0;
    let mut inserts: u64 = 0;

    let start = Instant::now();
    for &lba in &lbas {
        let result = engine.write(lba, buf.as_ref())?;
        if result.was_update {
            updates += 1;
        } else {
            inserts += 1;
        }
    }
    let elapsed = start.elapsed();

    Ok(RandomResult {
        operation: "Random Write".into(),
        cached: true,
        op_count,
        block_size,
        lba_range: max_lba,
        elapsed,
        cache_hits: updates,
        cache_misses: inserts,
    })
}
