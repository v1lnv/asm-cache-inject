//! Pretty-prints benchmark results in a compiler-diagnostic layout with color-coded speedup indicators.
//!
//! Example output:
//!
//! ```text
//!        Comparing comparison for `Seq Read`
//!       --> benchmark results
//!        |
//!        | Metric               |       Direct I/O |       Cached I/O |          Speedup
//!        | ---------------------|------------------|------------------|-----------------
//!        | Throughput           |       142.3 MB/s |      6712.1 MB/s |           47.2×
//!        | IOPS                 |            36429 |          1717017 |           47.1×
//!        | Avg Latency          |          27.5 μs |           0.6 μs |           45.8×
//!        | ---------------------|------------------|------------------|-----------------
//!        = note: cache hit ratio: 100.0% (1000 hits / 0 misses)
//!        |
//! ```

use colored::Colorize;

/// A single row in the benchmark report.
#[derive(Debug, Clone)]
pub struct ReportRow {
    pub label: String,
    pub throughput_mbps: f64,
    pub iops: f64,
    pub avg_latency_us: f64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

/// Print a pair of rows (direct vs cached) as a comparison table.
pub fn print_comparison(direct: &ReportRow, cached: &ReportRow) {
    let _lock = crate::cli::emit::PRINT_MUTEX.lock();
    let speedup = if direct.throughput_mbps > 0.0 {
        cached.throughput_mbps / direct.throughput_mbps
    } else {
        0.0
    };

    println!(
        "{:>12} comparison for `{}`",
        "Comparing".green().bold(),
        direct.label.replace(" (direct)", "").cyan().bold()
    );
    println!("{} benchmark results", "  -->".blue().bold());
    println!("{}", "   |".blue().bold());

    // Print table inside the gutter
    println!(
        "{} {:<20} | {:>15} | {:>15} | {:>15}",
        "   |".blue().bold(),
        "Metric".bold(),
        "Direct I/O".bold(),
        "Cached I/O".bold(),
        "Speedup".bold()
    );
    println!(
        "{} {:-<20}-|-{:-<15}-|-{:-<15}-|-{:-<15}",
        "   |".blue().bold(),
        "",
        "",
        "",
        ""
    );

    // Throughput row
    let speedup_str = format!("{speedup:.1}×");
    let speedup_coloured = if speedup >= 2.0 {
        speedup_str.green().bold()
    } else if speedup >= 1.0 {
        speedup_str.yellow()
    } else {
        speedup_str.red()
    };
    println!(
        "{} {:<20} | {:>10.1} MB/s | {:>10.1} MB/s | {:>15}",
        "   |".blue().bold(),
        "Throughput",
        direct.throughput_mbps,
        cached.throughput_mbps,
        speedup_coloured
    );

    // IOPS row
    let iops_speedup = if direct.iops > 0.0 {
        cached.iops / direct.iops
    } else {
        0.0
    };
    let iops_speedup_str = format!("{iops_speedup:.1}×");
    let iops_speedup_coloured = if iops_speedup >= 2.0 {
        iops_speedup_str.green().bold()
    } else if iops_speedup >= 1.0 {
        iops_speedup_str.yellow()
    } else {
        iops_speedup_str.red()
    };
    println!(
        "{} {:<20} | {:>15.0} | {:>15.0} | {:>15}",
        "   |".blue().bold(),
        "IOPS",
        direct.iops,
        cached.iops,
        iops_speedup_coloured
    );

    // Latency row
    let lat_speedup = if cached.avg_latency_us > 0.0 {
        direct.avg_latency_us / cached.avg_latency_us
    } else {
        0.0
    };
    let lat_speedup_str = format!("{lat_speedup:.1}×");
    let lat_speedup_coloured = if lat_speedup >= 2.0 {
        lat_speedup_str.green().bold()
    } else if lat_speedup >= 1.0 {
        lat_speedup_str.yellow()
    } else {
        lat_speedup_str.red()
    };
    println!(
        "{} {:<20} | {:>12.1} μs | {:>12.1} μs | {:>15}",
        "   |".blue().bold(),
        "Avg Latency",
        direct.avg_latency_us,
        cached.avg_latency_us,
        lat_speedup_coloured
    );

    println!(
        "{} {:-<20}-|-{:-<15}-|-{:-<15}-|-{:-<15}",
        "   |".blue().bold(),
        "",
        "",
        "",
        ""
    );

    // Cache hit ratio if cached has ops
    let total_ops = cached.cache_hits + cached.cache_misses;
    if total_ops > 0 {
        let hit_ratio = cached.cache_hits as f64 / total_ops as f64 * 100.0;
        println!(
            "{} {} {:.1}% ({} hits / {} misses)",
            "   = ".blue().bold(),
            "note: cache hit ratio:".bold(),
            hit_ratio,
            cached.cache_hits,
            cached.cache_misses,
        );
    }
    println!("{}", "   |".blue().bold());
    println!();
}

/// Print a summary header for the benchmark suite.
pub fn print_header(device_path: &str, cache_size_mb: usize, block_count: usize) {
    let _lock = crate::cli::emit::PRINT_MUTEX.lock();
    println!(
        "{:>12} asm-cache-inject-bench v{} ({})",
        "Benchmarking".green().bold(),
        env!("CARGO_PKG_VERSION"),
        device_path
    );
    println!("{} {}", "  -->".blue().bold(), device_path);
    println!("{}", "   |".blue().bold());
    println!(
        "{} {} starting benchmark run",
        "   = ".blue().bold(),
        "note:".bold()
    );
    println!(
        "{} {} RAM cache configuration:",
        "   = ".blue().bold(),
        "note:".bold()
    );
    println!(
        "{} {}: {} MiB",
        "   | ".blue().bold(),
        "Cache size".bold(),
        cache_size_mb
    );
    println!(
        "{} {}: {} blocks (4096 B/block)",
        "   | ".blue().bold(),
        "Block count".bold(),
        block_count
    );
    println!("{}", "   |".blue().bold());
}

/// Print a final summary line.
pub fn print_footer() {
    let _lock = crate::cli::emit::PRINT_MUTEX.lock();
    println!("{:>12} benchmark run complete", "Finished".green().bold());
    println!(
        "{} {}",
        "   |".blue().bold(),
        "All times are wall-clock elapsed.".dimmed()
    );
    println!();
}
