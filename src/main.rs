//! Entry point — parses CLI arguments, initialises the logger, and dispatches
//! to the appropriate command handler.

use clap::Parser;

use asm_cache_inject::cli::args::{Cli, Command};
use asm_cache_inject::cli::commands::{exec_bench, exec_cache, exec_info};

fn main() {
    let cli = Cli::parse();

    // Initialise logger based on verbosity level
    let log_level = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level))
        .format_timestamp_millis()
        .init();

    // Dispatch to subcommand
    let result = match &cli.command {
        Command::Cache(args) => exec_cache(args),
        Command::Bench(args) => exec_bench(args),
        Command::Info(args) => exec_info(args),
    };

    // Handle top-level errors
    if let Err(e) = result {
        asm_cache_inject::cli::emit::error(&e);
        std::process::exit(1);
    }
}
