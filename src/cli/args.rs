//! Command-line argument definitions using `clap` derive macros.
//!
//! Subcommands:
//!   cache  — Start or manage the cache engine.
//!   bench  — Run integrated benchmarks (direct vs cached I/O).
//!   info   — Display block device metadata.

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand};

fn help_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
        .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
        .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
        .placeholder(AnsiColor::Cyan.on_default())
}

/// `asm-cache-inject` — Low-level I/O cache engine with x86_64 ASM
/// acceleration for raw block devices (HDDs, SSDs, etc.).
#[derive(Parser, Debug)]
#[command(
    name = "asm-cache-inject",
    version = env!("CARGO_PKG_VERSION"),
    about = "RAM cache accelerator for block devices using x86_64 ASM streaming instructions",
    long_about = None,
    styles = help_styles()
)]
pub struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Command,

    /// Enable verbose logging (repeat for more: -v = info, -vv = debug,
    /// -vvv = trace).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
}

/// Top-level subcommands.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Start the cache engine for a block device.
    Cache(CacheArgs),

    /// Run I/O benchmarks comparing direct vs cached performance.
    Bench(BenchArgs),

    /// Display information about a block device.
    Info(InfoArgs),
}

/// Arguments for the `cache` subcommand.
#[derive(Parser, Debug)]
pub struct CacheArgs {
    /// Path to the block device (e.g. /dev/sda, /dev/sdb1).
    #[arg(short, long)]
    pub device: String,

    /// Cache size in megabytes (default: 256).
    #[arg(short = 's', long, default_value_t = 256)]
    pub cache_size: usize,

    /// Block size in bytes (must be a multiple of 4096, default: 4096).
    #[arg(short, long, default_value_t = 4096)]
    pub block_size: usize,

    /// Background flush interval in seconds (default: 5).
    #[arg(short, long, default_value_t = 5)]
    pub flush_interval: u64,

    /// Dirty watermark (0.0–1.0) that triggers immediate flush (default: 0.75).
    #[arg(short = 'w', long, default_value_t = 0.75)]
    pub dirty_watermark: f64,

    /// Open device in read-only mode (safe for benchmarking).
    #[arg(short, long, default_value_t = false)]
    pub read_only: bool,
}

/// Arguments for the `bench` subcommand.
#[derive(Parser, Debug)]
pub struct BenchArgs {
    /// Path to the block device to benchmark.
    #[arg(short, long)]
    pub device: String,

    /// Benchmark mode: sequential, random, or all.
    #[arg(short, long, default_value = "all")]
    pub mode: String,

    /// Number of blocks / operations per benchmark run (default: 1000).
    #[arg(short = 'n', long, default_value_t = 1000)]
    pub block_count: usize,

    /// Cache size in megabytes for the cached benchmark (default: 256).
    #[arg(short = 's', long, default_value_t = 256)]
    pub cache_size: usize,

    /// Include write benchmarks (WARNING: destructive — overwrites data).
    #[arg(long, default_value_t = false)]
    pub confirm_write: bool,
}

/// Arguments for the `info` subcommand.
#[derive(Parser, Debug)]
pub struct InfoArgs {
    /// Path to the block device to inspect.
    #[arg(short, long)]
    pub device: String,
}
