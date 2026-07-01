//! Configuration for the cache engine. All user-facing parameters are
//! collected here and validated before the engine starts.

use std::path::{Path, PathBuf};

use crate::error::{CacheError, CacheResult};
use crate::memory::PAGE_ALIGNMENT;

/// Default cache size in bytes (256 MiB).
pub const DEFAULT_CACHE_SIZE_MB: usize = 256;

/// Default block size in bytes (one OS page).
pub const DEFAULT_BLOCK_SIZE: usize = 4096;

/// Default flush interval in seconds.
pub const DEFAULT_FLUSH_INTERVAL_SECS: u64 = 5;

/// Default dirty watermark (75 %).
pub const DEFAULT_DIRTY_WATERMARK: f64 = 0.75;

/// Engine configuration struct.
///
/// Construct via [`CacheConfig::builder()`] or directly, then call
/// [`validate()`](CacheConfig::validate) before passing to the engine.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Path to the target block device (e.g. `/dev/sda`).
    pub device_path: PathBuf,

    /// Total RAM to allocate for the cache, in megabytes.
    pub cache_size_mb: usize,

    /// Size of each cache block in bytes. Must be a multiple of
    /// [`PAGE_ALIGNMENT`].
    pub block_size: usize,

    /// Interval between periodic flush cycles, in seconds.
    pub flush_interval_secs: u64,

    /// Fraction of dirty blocks that triggers an immediate flush (0.0–1.0).
    pub dirty_watermark: f64,

    /// If `true`, the device is opened read-only and write operations are
    /// rejected. Safe mode for benchmarking.
    pub read_only: bool,
}

impl CacheConfig {
    /// Create a config for the given device with sensible defaults.
    pub fn new(device_path: impl AsRef<Path>) -> Self {
        Self {
            device_path: device_path.as_ref().to_path_buf(),
            cache_size_mb: DEFAULT_CACHE_SIZE_MB,
            block_size: DEFAULT_BLOCK_SIZE,
            flush_interval_secs: DEFAULT_FLUSH_INTERVAL_SECS,
            dirty_watermark: DEFAULT_DIRTY_WATERMARK,
            read_only: false,
        }
    }

    /// Total cache size in bytes.
    pub fn cache_size_bytes(&self) -> usize {
        self.cache_size_mb * 1024 * 1024
    }

    /// Number of blocks that fit in the cache.
    pub fn block_count(&self) -> usize {
        self.cache_size_bytes() / self.block_size
    }

    /// Validate all parameters and return `Ok(())` or a descriptive error.
    pub fn validate(&self) -> CacheResult<()> {
        if self.device_path.as_os_str().is_empty() {
            return Err(CacheError::config("device_path must not be empty"));
        }

        if self.cache_size_mb == 0 {
            return Err(CacheError::config("cache_size_mb must be > 0"));
        }

        if self.block_size == 0 || !self.block_size.is_multiple_of(PAGE_ALIGNMENT) {
            return Err(CacheError::config(format!(
                "block_size ({}) must be a non-zero multiple of {PAGE_ALIGNMENT}",
                self.block_size,
            )));
        }

        if !self.cache_size_bytes().is_multiple_of(self.block_size) {
            return Err(CacheError::config(format!(
                "cache_size ({} MiB = {} bytes) must be a multiple of block_size ({})",
                self.cache_size_mb,
                self.cache_size_bytes(),
                self.block_size,
            )));
        }

        if !(0.0..=1.0).contains(&self.dirty_watermark) {
            return Err(CacheError::config(format!(
                "dirty_watermark ({}) must be in [0.0, 1.0]",
                self.dirty_watermark,
            )));
        }

        if self.flush_interval_secs == 0 {
            return Err(CacheError::config("flush_interval_secs must be > 0"));
        }

        Ok(())
    }
}

impl std::fmt::Display for CacheConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CacheConfig {{\n  device:     {}\n  cache:      {} MiB ({} blocks × {} B)\n  flush:      every {} s (watermark {:.0}%)\n  read_only:  {}\n}}",
            self.device_path.display(),
            self.cache_size_mb,
            self.block_count(),
            self.block_size,
            self.flush_interval_secs,
            self.dirty_watermark * 100.0,
            self.read_only,
        )
    }
}

// Unit tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_config() {
        let cfg = CacheConfig::new("/dev/sda");
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.block_count(), 65536); // 256 MiB / 4096
    }

    #[test]
    fn rejects_zero_cache_size() {
        let mut cfg = CacheConfig::new("/dev/sda");
        cfg.cache_size_mb = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_bad_block_size() {
        let mut cfg = CacheConfig::new("/dev/sda");
        cfg.block_size = 1000;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_bad_watermark() {
        let mut cfg = CacheConfig::new("/dev/sda");
        cfg.dirty_watermark = 1.5;
        assert!(cfg.validate().is_err());
    }
}
