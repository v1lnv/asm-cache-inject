//! Queries metadata about a block device: total size, sector size, and
//! hardware model string. This information is used to:
//!
//!   - Validate that the target is actually a block device.
//!   - Calculate the maximum addressable LBA.
//!   - Choose the correct I/O alignment for O_DIRECT.
//!   - Display device details in the CLI `info` subcommand.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{CacheError, CacheResult};

/// Metadata about a block device.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Canonical path to the device (e.g. `/dev/sda`).
    pub path: PathBuf,
    /// Total device size in bytes.
    pub total_bytes: u64,
    /// Logical sector size in bytes (typically 512 or 4096).
    pub sector_size: u64,
    /// Total number of addressable sectors.
    pub sector_count: u64,
    /// Hardware model string (from sysfs), or "unknown" if unavailable.
    pub model: String,
}

impl DeviceInfo {
    /// Query metadata for the block device at `path`.
    ///
    /// # Errors
    ///
    /// - [`CacheError::DeviceNotFound`] if the path does not exist.
    /// - [`CacheError::NotBlockDevice`] if the path is not a block device.
    /// - [`CacheError::Io`] if an ioctl or sysfs read fails.
    pub fn query(path: &Path) -> CacheResult<Self> {
        // Validate path exists
        if !path.exists() {
            return Err(CacheError::DeviceNotFound {
                path: path.to_path_buf(),
            });
        }

        // Validate it is a block device
        let metadata = fs::metadata(path).map_err(|e| CacheError::io("stat", e))?;
        if !is_block_device(&metadata) {
            return Err(CacheError::NotBlockDevice {
                path: path.to_path_buf(),
            });
        }

        // Open the device to query size via ioctl
        let fd = open_readonly(path)?;
        let total_bytes = ioctl_get_size(fd)?;
        let sector_size = ioctl_get_sector_size(fd)?;
        close_fd(fd);

        let sector_count = total_bytes.checked_div(sector_size).unwrap_or(0);

        // Read model from sysfs
        let model = read_device_model(path);

        Ok(Self {
            path: path.to_path_buf(),
            total_bytes,
            sector_size,
            sector_count,
            model,
        })
    }

    /// Maximum addressable LBA for a given `block_size`.
    pub fn max_lba(&self, block_size: u64) -> u64 {
        if block_size == 0 {
            return 0;
        }
        self.total_bytes / block_size
    }
}

impl std::fmt::Display for DeviceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size_gib = self.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        write!(
            f,
            "Device: {}\n  Model:       {}\n  Size:        {:.2} GiB ({} bytes)\n  Sector size: {} bytes\n  Sectors:     {}",
            self.path.display(),
            self.model,
            size_gib,
            self.total_bytes,
            self.sector_size,
            self.sector_count,
        )
    }
}

// Platform helpers (Linux-specific)

/// Check whether the file metadata indicates a block device.
#[cfg(unix)]
fn is_block_device(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::FileTypeExt;
    meta.file_type().is_block_device()
}

#[cfg(not(unix))]
fn is_block_device(_meta: &fs::Metadata) -> bool {
    false
}

/// Open a block device read-only and return the raw file descriptor.
fn open_readonly(path: &Path) -> CacheResult<i32> {
    use std::ffi::CString;
    let c_path = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| CacheError::config(format!("path contains null byte: {}", path.display())))?;
    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY) };
    if fd < 0 {
        return Err(CacheError::io(
            "open (read-only)",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(fd)
}

/// Close a file descriptor (ignoring errors).
fn close_fd(fd: i32) {
    unsafe {
        libc::close(fd);
    }
}

/// `ioctl(fd, BLKGETSIZE64, &size)` — returns device size in bytes.
fn ioctl_get_size(fd: i32) -> CacheResult<u64> {
    // BLKGETSIZE64 = 0x80081272 on Linux (from <linux/fs.h>).
    const BLKGETSIZE64: libc::c_ulong = 0x80081272;
    let mut size: u64 = 0;
    let ret = unsafe { libc::ioctl(fd, BLKGETSIZE64, &mut size) };
    if ret < 0 {
        return Err(CacheError::io(
            "ioctl BLKGETSIZE64",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(size)
}

/// `ioctl(fd, BLKSSZGET, &sector_size)` — returns logical sector size.
fn ioctl_get_sector_size(fd: i32) -> CacheResult<u64> {
    // BLKSSZGET = 0x1268 on Linux.
    const BLKSSZGET: libc::c_ulong = 0x1268;
    let mut ss: libc::c_int = 0;
    let ret = unsafe { libc::ioctl(fd, BLKSSZGET, &mut ss) };
    if ret < 0 {
        return Err(CacheError::io(
            "ioctl BLKSSZGET",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(ss as u64)
}

/// Attempt to read the device model from sysfs.
///
/// For a device like `/dev/sda` we look at `/sys/block/sda/device/model`.
/// Returns `"unknown"` if the sysfs entry does not exist (e.g. loop devices).
fn read_device_model(path: &Path) -> String {
    let dev_name = match path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name,
        None => return "unknown".to_string(),
    };

    let sysfs_path = format!("/sys/block/{dev_name}/device/model");
    fs::read_to_string(&sysfs_path)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}
