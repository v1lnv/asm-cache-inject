//! Benchmark orchestrator — coordinates running direct and cached benchmarks,
//! collects results, and prints the comparison report.

use crate::engine::{CacheConfig, CacheEngine};
use crate::error::CacheResult;
use crate::io::BlockDevice;

use super::random::{
    bench_random_read_cached, bench_random_read_direct, bench_random_write_cached,
    bench_random_write_direct,
};
use super::report::{print_comparison, print_footer, print_header, ReportRow};
use super::sequential::{
    bench_sequential_read_cached, bench_sequential_read_direct, bench_sequential_write_cached,
    bench_sequential_write_direct,
};

/// Benchmark mode selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchMode {
    /// Sequential read/write.
    Sequential,
    /// Random read/write.
    Random,
    /// Both sequential and random.
    All,
}

impl std::str::FromStr for BenchMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sequential" | "seq" => Ok(Self::Sequential),
            "random" | "rand" => Ok(Self::Random),
            "all" => Ok(Self::All),
            _ => Err(format!("unknown bench mode: {s}")),
        }
    }
}

/// Configuration for a benchmark run.
#[derive(Debug, Clone)]
pub struct BenchConfig {
    /// Path to the block device.
    pub device_path: String,
    /// Benchmark mode (sequential, random, or all).
    pub mode: BenchMode,
    /// Number of blocks / operations per benchmark.
    pub block_count: usize,
    /// Cache size in MiB.
    pub cache_size_mb: usize,
    /// Whether to run write benchmarks (requires --confirm-write).
    pub include_writes: bool,
}

/// Run the full benchmark suite.
pub fn run_benchmarks(bench_config: &BenchConfig) -> CacheResult<()> {
    let device_path = &bench_config.device_path;
    let block_count = bench_config.block_count;
    let cache_size_mb = bench_config.cache_size_mb;

    print_header(device_path, cache_size_mb, block_count);

    // Open the device directly for "no cache" baselines
    let direct_device = BlockDevice::open(std::path::Path::new(device_path), true)?;
    let fd = direct_device.fd();
    let block_size = 4096_usize;
    let max_lba = direct_device.info().max_lba(block_size as u64);

    // Clamp block_count to device size.
    let effective_count = block_count.min(max_lba as usize);

    // Create a cached engine for comparison
    let mut engine_config = CacheConfig::new(device_path);
    engine_config.cache_size_mb = cache_size_mb;
    engine_config.read_only = !bench_config.include_writes;

    let mut engine = CacheEngine::new(engine_config)?;
    engine.start();

    // Sequential benchmarks
    if bench_config.mode == BenchMode::Sequential || bench_config.mode == BenchMode::All {
        crate::cli::emit::status("Running", "Sequential Read benchmark");

        let direct_seq_read = bench_sequential_read_direct(fd, block_size, effective_count)?;
        let cached_seq_read = bench_sequential_read_cached(&engine, effective_count)?;

        print_comparison(
            &ReportRow {
                label: "Seq Read (direct)".into(),
                throughput_mbps: direct_seq_read.throughput_mbps(),
                iops: direct_seq_read.iops(),
                avg_latency_us: direct_seq_read.avg_latency_us(),
                cache_hits: 0,
                cache_misses: direct_seq_read.block_count as u64,
            },
            &ReportRow {
                label: "Seq Read (cached)".into(),
                throughput_mbps: cached_seq_read.throughput_mbps(),
                iops: cached_seq_read.iops(),
                avg_latency_us: cached_seq_read.avg_latency_us(),
                cache_hits: cached_seq_read.cache_hits,
                cache_misses: cached_seq_read.cache_misses,
            },
        );

        // Run the cached read again to show warm-cache performance.
        crate::cli::emit::status("Running", "Sequential Read (warm cache) benchmark");
        let warm_seq_read = bench_sequential_read_cached(&engine, effective_count)?;

        print_comparison(
            &ReportRow {
                label: "Seq Read (direct)".into(),
                throughput_mbps: direct_seq_read.throughput_mbps(),
                iops: direct_seq_read.iops(),
                avg_latency_us: direct_seq_read.avg_latency_us(),
                cache_hits: 0,
                cache_misses: direct_seq_read.block_count as u64,
            },
            &ReportRow {
                label: "Seq Read (warm cache)".into(),
                throughput_mbps: warm_seq_read.throughput_mbps(),
                iops: warm_seq_read.iops(),
                avg_latency_us: warm_seq_read.avg_latency_us(),
                cache_hits: warm_seq_read.cache_hits,
                cache_misses: warm_seq_read.cache_misses,
            },
        );

        // Write benchmarks (only if confirmed).
        if bench_config.include_writes {
            crate::cli::emit::status("Running", "Sequential Write benchmark");

            let direct_seq_write = bench_sequential_write_direct(fd, block_size, effective_count)?;
            let cached_seq_write = bench_sequential_write_cached(&engine, effective_count)?;

            print_comparison(
                &ReportRow {
                    label: "Seq Write (direct)".into(),
                    throughput_mbps: direct_seq_write.throughput_mbps(),
                    iops: direct_seq_write.iops(),
                    avg_latency_us: direct_seq_write.avg_latency_us(),
                    cache_hits: 0,
                    cache_misses: 0,
                },
                &ReportRow {
                    label: "Seq Write (cached)".into(),
                    throughput_mbps: cached_seq_write.throughput_mbps(),
                    iops: cached_seq_write.iops(),
                    avg_latency_us: cached_seq_write.avg_latency_us(),
                    cache_hits: cached_seq_write.cache_hits,
                    cache_misses: cached_seq_write.cache_misses,
                },
            );
        }
    }

    // Random benchmarks
    if bench_config.mode == BenchMode::Random || bench_config.mode == BenchMode::All {
        crate::cli::emit::status("Running", "Random Read benchmark");

        let direct_rnd_read = bench_random_read_direct(fd, block_size, effective_count, max_lba)?;
        let cached_rnd_read = bench_random_read_cached(&engine, effective_count, max_lba)?;

        print_comparison(
            &ReportRow {
                label: "Rnd Read (direct)".into(),
                throughput_mbps: direct_rnd_read.throughput_mbps(),
                iops: direct_rnd_read.iops(),
                avg_latency_us: direct_rnd_read.avg_latency_us(),
                cache_hits: 0,
                cache_misses: direct_rnd_read.op_count as u64,
            },
            &ReportRow {
                label: "Rnd Read (cached)".into(),
                throughput_mbps: cached_rnd_read.throughput_mbps(),
                iops: cached_rnd_read.iops(),
                avg_latency_us: cached_rnd_read.avg_latency_us(),
                cache_hits: cached_rnd_read.cache_hits,
                cache_misses: cached_rnd_read.cache_misses,
            },
        );

        // Warm cache random read (same seed = same LBAs = all hits).
        crate::cli::emit::status("Running", "Random Read (warm cache) benchmark");
        let warm_rnd_read = bench_random_read_cached(&engine, effective_count, max_lba)?;

        print_comparison(
            &ReportRow {
                label: "Rnd Read (direct)".into(),
                throughput_mbps: direct_rnd_read.throughput_mbps(),
                iops: direct_rnd_read.iops(),
                avg_latency_us: direct_rnd_read.avg_latency_us(),
                cache_hits: 0,
                cache_misses: direct_rnd_read.op_count as u64,
            },
            &ReportRow {
                label: "Rnd Read (warm cache)".into(),
                throughput_mbps: warm_rnd_read.throughput_mbps(),
                iops: warm_rnd_read.iops(),
                avg_latency_us: warm_rnd_read.avg_latency_us(),
                cache_hits: warm_rnd_read.cache_hits,
                cache_misses: warm_rnd_read.cache_misses,
            },
        );

        if bench_config.include_writes {
            crate::cli::emit::status("Running", "Random Write benchmark");

            let direct_rnd_write =
                bench_random_write_direct(fd, block_size, effective_count, max_lba)?;
            let cached_rnd_write = bench_random_write_cached(&engine, effective_count, max_lba)?;

            print_comparison(
                &ReportRow {
                    label: "Rnd Write (direct)".into(),
                    throughput_mbps: direct_rnd_write.throughput_mbps(),
                    iops: direct_rnd_write.iops(),
                    avg_latency_us: direct_rnd_write.avg_latency_us(),
                    cache_hits: 0,
                    cache_misses: 0,
                },
                &ReportRow {
                    label: "Rnd Write (cached)".into(),
                    throughput_mbps: cached_rnd_write.throughput_mbps(),
                    iops: cached_rnd_write.iops(),
                    avg_latency_us: cached_rnd_write.avg_latency_us(),
                    cache_hits: cached_rnd_write.cache_hits,
                    cache_misses: cached_rnd_write.cache_misses,
                },
            );
        }
    }

    // Shutdown
    engine.stop()?;
    print_footer();

    Ok(())
}
