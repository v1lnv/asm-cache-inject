//! Command dispatch logic — maps parsed CLI arguments to engine / benchmark /
//! info actions.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use colored::Colorize;

use crate::bench::{run_benchmarks, BenchConfig, BenchMode};
use crate::engine::{CacheConfig, CacheEngine};
use crate::error::CacheResult;
use crate::io::DeviceInfo;

use super::args::{BenchArgs, CacheArgs, InfoArgs};
use super::emit;

/// Execute the `cache` subcommand — starts the cache engine and keeps it
/// running until the user presses Ctrl+C.
pub fn exec_cache(args: &CacheArgs) -> CacheResult<()> {
    let config = CacheConfig {
        device_path: args.device.clone().into(),
        cache_size_mb: args.cache_size,
        block_size: args.block_size,
        flush_interval_secs: args.flush_interval,
        dirty_watermark: args.dirty_watermark,
        read_only: args.read_only,
    };

    let mut engine = CacheEngine::new(config)?;
    engine.start();

    emit::status("Injecting", &format!("cache-engine onto `{}`", args.device));

    {
        let _lock = emit::PRINT_MUTEX.lock();
        println!("{} {}", "  -->".blue().bold(), args.device);
        println!("{}", "   |".blue().bold());
        println!("  1 | CacheConfig {{");
        println!("  2 |   {:<17} {} MiB,", "cache_size:", args.cache_size);
        println!("  3 |   {:<17} {} B,", "block_size:", args.block_size);
        println!(
            "  4 |   {:<17} {} s,",
            "flush_interval:", args.flush_interval
        );
        println!(
            "  5 |   {:<17} {:.0}%,",
            "dirty_watermark:",
            args.dirty_watermark * 100.0
        );
        println!("  6 |   {:<17} {},", "read_only:", args.read_only);
        println!("  7 | }}");
        println!("{}", "   |".blue().bold());
        println!(
            "{} {} cache engine successfully running",
            "   = ".blue().bold(),
            "note:".bold()
        );
        println!(
            "{} {} press `Ctrl+C` to gracefully shut down and commit all dirty blocks",
            "   = ".blue().bold(),
            "help:".bold()
        );
        println!();
    }

    // Wait for SIGINT / SIGTERM
    let running = Arc::new(AtomicBool::new(true));
    let r = Arc::clone(&running);

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("failed to set Ctrl+C handler");

    while running.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    emit::status("Stopping", "cache engine");
    {
        let _lock = emit::PRINT_MUTEX.lock();
        println!("{} {}", "  -->".blue().bold(), args.device);
        println!("{}", "   |".blue().bold());
        println!(
            "{} {} flushing all remaining dirty blocks from RAM cache to disk",
            "   = ".blue().bold(),
            "note:".bold()
        );
    }

    engine.stop()?;

    emit::status("Finished", "shutdown cleanly (all blocks committed)");
    println!();

    Ok(())
}

/// Execute the `bench` subcommand — run integrated benchmarks.
pub fn exec_bench(args: &BenchArgs) -> CacheResult<()> {
    let mode: BenchMode = args
        .mode
        .parse()
        .map_err(|e: String| crate::error::CacheError::config(e))?;

    if args.confirm_write {
        emit::warning(
            "destructive write benchmarks enabled",
            Some(Path::new(&args.device)),
            &[
                "caution: data on the device WILL be overwritten!",
                "help: ensure you have backed up any important data before proceeding.",
            ],
        );
    }

    let bench_config = BenchConfig {
        device_path: args.device.clone(),
        mode,
        block_count: args.block_count,
        cache_size_mb: args.cache_size,
        include_writes: args.confirm_write,
    };

    run_benchmarks(&bench_config)
}

/// Execute the `info` subcommand — display device metadata.
pub fn exec_info(args: &InfoArgs) -> CacheResult<()> {
    let path = Path::new(&args.device);
    let info = DeviceInfo::query(path)?;

    emit::device_info(&info);

    Ok(())
}
