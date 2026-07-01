//! Thread-safe emitter for printing formatted output in the style of the Rust
//! compiler (rustc). Provides helpers for status messages, warnings, errors,
//! block device info, and benchmark reports.

use colored::Colorize;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};

use crate::error::CacheError;

/// Global print lock to guarantee that all diagnostics/status outputs are
/// written atomically, avoiding race conditions in multi-threaded environments.
pub static PRINT_MUTEX: Mutex<()> = Mutex::new(());

/// Maps CacheError variants to unique compiler-style error codes.
pub fn get_error_code(err: &CacheError) -> &'static str {
    match err {
        CacheError::Config { .. } => "E0001",
        CacheError::DeviceNotFound { .. } => "E0002",
        CacheError::NotBlockDevice { .. } => "E0003",
        CacheError::Allocation { .. } => "E0004",
        CacheError::Alignment { .. } => "E0005",
        CacheError::PoolExhausted { .. } => "E0006",
        CacheError::Io { .. } => "E0007",
        CacheError::Ioctl { .. } => "E0008",
        CacheError::FlushThread { .. } => "E0009",
        CacheError::Internal { .. } => "E0010",
        CacheError::LbaOutOfRange { .. } => "E0011",
        CacheError::PermissionDenied { .. } => "E0012",
    }
}

/// Helper to resolve the virtual "source file" path for the compiler diagnostics.
fn get_error_path(err: &CacheError) -> PathBuf {
    match err {
        CacheError::DeviceNotFound { path } => path.clone(),
        CacheError::NotBlockDevice { path } => path.clone(),
        CacheError::PermissionDenied { .. } => Path::new("device").to_path_buf(),
        CacheError::Config { .. } => Path::new("config").to_path_buf(),
        CacheError::Allocation { .. } => Path::new("memory").to_path_buf(),
        CacheError::Alignment { .. } => Path::new("memory").to_path_buf(),
        CacheError::PoolExhausted { .. } => Path::new("cache").to_path_buf(),
        CacheError::Io { .. } => Path::new("io").to_path_buf(),
        CacheError::Ioctl { .. } => Path::new("io").to_path_buf(),
        CacheError::FlushThread { .. } => Path::new("flush").to_path_buf(),
        CacheError::Internal { .. } => Path::new("internal").to_path_buf(),
        CacheError::LbaOutOfRange { .. } => Path::new("io").to_path_buf(),
    }
}

/// Emit status in cargo/rustc format: 12-char right-aligned bold green label.
pub fn status(action: &str, msg: &str) {
    let _lock = PRINT_MUTEX.lock();
    println!("{:>12} {}", action.green().bold(), msg);
}

/// Emit a compiler-style warning diagnostic.
pub fn warning(msg: &str, device_path: Option<&Path>, help_notes: &[&str]) {
    let _lock = PRINT_MUTEX.lock();
    eprintln!(
        "{}{} {}",
        "warning".yellow().bold(),
        ":".white().bold(),
        msg.white().bold()
    );
    if let Some(path) = device_path {
        eprintln!("{} {}", "  -->".blue().bold(), path.display());
    }
    eprintln!("{}", "   |".blue().bold());
    for note in help_notes {
        eprintln!("{} {}", "   = ".blue().bold(), note);
    }
    eprintln!();
}

/// Emit detailed compiler-style error with code mapping and notes.
pub fn error(err: &CacheError) {
    let _lock = PRINT_MUTEX.lock();
    let code = get_error_code(err);
    let err_msg = err.to_string();

    eprintln!(
        "{}{} {}{}",
        "error[".red().bold(),
        code.red().bold(),
        "]: ".red().bold(),
        err_msg.white().bold()
    );
    eprintln!(
        "{} {}",
        "  -->".blue().bold(),
        get_error_path(err).display()
    );
    eprintln!("{}", "   |".blue().bold());

    match err {
        CacheError::DeviceNotFound { path } => {
            eprintln!(
                "{} {}: {}",
                "   |".blue().bold(),
                "path".yellow().bold(),
                path.display()
            );
            eprintln!("{}", "   |".blue().bold());
            eprintln!(
                "{} {} make sure the device is plugged in and check the path via `lsblk`",
                "   = ".blue().bold(),
                "note:".bold()
            );
        }
        CacheError::NotBlockDevice { path } => {
            eprintln!(
                "{} {}: {}",
                "   |".blue().bold(),
                "path".yellow().bold(),
                path.display()
            );
            eprintln!("{}", "   |".blue().bold());
            eprintln!(
                "{} {} target path exists but is not a hardware block device",
                "   = ".blue().bold(),
                "note:".bold()
            );
            eprintln!(
                "{} {} only raw block devices (e.g., /dev/sdX, /dev/loopX) are supported",
                "   = ".blue().bold(),
                "help:".bold()
            );
        }
        CacheError::PermissionDenied { detail } => {
            eprintln!("{} {}", "   |".blue().bold(), detail.red());
            eprintln!("{}", "   |".blue().bold());
            eprintln!(
                "{} {} reading or writing to raw block devices requires root privileges",
                "   = ".blue().bold(),
                "note:".bold()
            );
            eprintln!(
                "{} {} try running with `sudo` or check if the user has `CAP_SYS_RAWIO` capability",
                "   = ".blue().bold(),
                "help:".bold()
            );
        }
        CacheError::Config { message } => {
            eprintln!("{} {}", "   |".blue().bold(), message.red());
            eprintln!("{}", "   |".blue().bold());
            eprintln!(
                "{} {} verify the command-line arguments",
                "   = ".blue().bold(),
                "note:".bold()
            );
        }
        CacheError::Allocation { reason } => {
            eprintln!("{} {}", "   |".blue().bold(), reason.red());
            eprintln!("{}", "   |".blue().bold());
            eprintln!(
                "{} {} system is out of memory or the memory alignment request failed",
                "   = ".blue().bold(),
                "note:".bold()
            );
        }
        CacheError::Alignment { expected, actual } => {
            eprintln!(
                "{} expected alignment {}, got offset {}",
                "   |".blue().bold(),
                expected,
                actual
            );
            eprintln!("{}", "   |".blue().bold());
            eprintln!(
                "{} {} block device operations require O_DIRECT buffers to be page-aligned",
                "   = ".blue().bold(),
                "note:".bold()
            );
        }
        CacheError::PoolExhausted { in_use, capacity } => {
            eprintln!(
                "{} {} of {} blocks in use",
                "   |".blue().bold(),
                in_use,
                capacity
            );
            eprintln!("{}", "   |".blue().bold());
            eprintln!(
                "{} {} all memory pool blocks are dirty and background flusher is catching up",
                "   = ".blue().bold(),
                "note:".bold()
            );
            eprintln!(
                "{} {} increase flush frequency or cache size",
                "   = ".blue().bold(),
                "help:".bold()
            );
        }
        CacheError::Io { operation, source } => {
            eprintln!(
                "{} failed during {} ({})",
                "   |".blue().bold(),
                operation,
                source
            );
            eprintln!("{}", "   |".blue().bold());
            eprintln!(
                "{} {} underlying OS system call returned an error",
                "   = ".blue().bold(),
                "note:".bold()
            );
        }
        CacheError::Ioctl { name, detail } => {
            eprintln!("{} ioctl {} failed: {}", "   |".blue().bold(), name, detail);
            eprintln!("{}", "   |".blue().bold());
            eprintln!(
                "{} {} target block device may not support this operation or format",
                "   = ".blue().bold(),
                "note:".bold()
            );
        }
        CacheError::FlushThread { reason } => {
            eprintln!("{} flusher failed: {}", "   |".blue().bold(), reason);
            eprintln!("{}", "   |".blue().bold());
            eprintln!(
                "{} {} background flusher encountered an unrecoverable panic",
                "   = ".blue().bold(),
                "note:".bold()
            );
        }
        CacheError::Internal { message } => {
            eprintln!("{} invariant violated: {}", "   |".blue().bold(), message);
            eprintln!("{}", "   |".blue().bold());
            eprintln!(
                "{} {} this is a bug in the cache engine; please report it",
                "   = ".blue().bold(),
                "note:".bold()
            );
        }
        CacheError::LbaOutOfRange { lba, max_lba } => {
            eprintln!(
                "{} LBA {} is out of range (max: {})",
                "   |".blue().bold(),
                lba,
                max_lba
            );
            eprintln!("{}", "   |".blue().bold());
            eprintln!(
                "{} {} cannot access an offset beyond the physical device capacity",
                "   = ".blue().bold(),
                "note:".bold()
            );
        }
    }
    eprintln!();
}

/// Print block device metadata in the style of compiler notes.
pub fn device_info(info: &crate::io::DeviceInfo) {
    let _lock = PRINT_MUTEX.lock();
    let size_gib = info.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    println!(
        "{:>12} block-device `/dev/{}`",
        "Introspecting".green().bold(),
        info.path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("unknown"))
            .to_string_lossy()
    );
    println!("{} {}", "  -->".blue().bold(), info.path.display());
    println!("{}", "   |".blue().bold());
    println!(
        "{} {} hardware block device details:",
        "   = ".blue().bold(),
        "note:".bold()
    );
    println!(
        "{} {}: {}",
        "   | ".blue().bold(),
        format!("{:<11}", "Model").bold(),
        info.model
    );
    println!(
        "{} {}: {:.2} GiB ({} bytes)",
        "   | ".blue().bold(),
        format!("{:<11}", "Size").bold(),
        size_gib,
        info.total_bytes
    );
    println!(
        "{} {}: {} bytes",
        "   | ".blue().bold(),
        format!("{:<11}", "Sector size").bold(),
        info.sector_size
    );
    println!(
        "{} {}: {}",
        "   | ".blue().bold(),
        format!("{:<11}", "Sectors").bold(),
        info.sector_count
    );
    println!(
        "{} {}: {}",
        "   | ".blue().bold(),
        format!("{:<11}", "Max LBA").bold(),
        info.max_lba(4096)
    );
    println!("{}", "   |".blue().bold());
}
