//! Centralised error types for the entire crate. Every module propagates errors
//! through this single enum so call-sites can pattern-match on a unified type.

use std::path::PathBuf;

/// Top-level error type for the asm-cache-inject cache engine.
///
/// All fallible operations across the crate return `Result<T, CacheError>`.
/// Variants are ordered by severity — from configuration mistakes through
/// runtime I/O failures to internal invariant violations.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    // Configuration & Validation
    /// The supplied configuration contains an invalid or contradictory value.
    #[error("Configuration error: {message}")]
    Config { message: String },

    /// The device path does not exist on the filesystem.
    #[error("Device path does not exist: {path}")]
    DeviceNotFound { path: PathBuf },

    /// The path exists but is not a block device (e.g. a regular file).
    #[error("Path is not a block device: {path}")]
    NotBlockDevice { path: PathBuf },

    // Memory Allocation
    /// Page-aligned memory allocation failed (out of memory or bad layout).
    #[error("Memory allocation failed: {reason}")]
    Allocation { reason: String },

    /// A pointer or buffer does not meet the required alignment.
    #[error("Alignment violation: expected {expected}-byte alignment, got offset {actual}")]
    Alignment { expected: usize, actual: usize },

    /// The cache memory pool has no free blocks available.
    #[error("Cache pool exhausted ({in_use} of {capacity} blocks in use)")]
    PoolExhausted { in_use: usize, capacity: usize },

    // I/O Errors
    /// A low-level I/O system call failed (pread, pwrite, open, ioctl, etc.).
    #[error("I/O error on {operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },

    /// An `ioctl` call returned an unexpected value.
    #[error("ioctl {name} failed: {detail}")]
    Ioctl { name: &'static str, detail: String },

    // Runtime / Internal
    /// The background flush thread panicked or could not be joined.
    #[error("Flush thread error: {reason}")]
    FlushThread { reason: String },

    /// A cache-internal invariant was violated (should never happen).
    #[error("Internal error: {message}")]
    Internal { message: String },

    /// The requested LBA is outside the addressable range of the device.
    #[error("LBA {lba} is out of range (device has {max_lba} blocks)")]
    LbaOutOfRange { lba: u64, max_lba: u64 },

    /// Permission denied when accessing the block device.
    #[error("Permission denied: {detail}. Try running with sudo or CAP_SYS_RAWIO.")]
    PermissionDenied { detail: String },
}

/// Convenience alias used throughout the crate.
pub type CacheResult<T> = Result<T, CacheError>;

// Conversion helpers

impl CacheError {
    /// Wrap a `std::io::Error` with contextual information about which
    /// operation triggered it.
    pub fn io(operation: &'static str, source: std::io::Error) -> Self {
        // Automatically promote EACCES / EPERM to a friendlier variant.
        if source.kind() == std::io::ErrorKind::PermissionDenied {
            return Self::PermissionDenied {
                detail: format!("{operation}: {source}"),
            };
        }
        Self::Io { operation, source }
    }

    /// Shorthand for a configuration validation failure.
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config {
            message: msg.into(),
        }
    }

    /// Shorthand for an internal invariant violation.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal {
            message: msg.into(),
        }
    }
}
