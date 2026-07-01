//! Opens a block device with `O_DIRECT` for cache-bypassing I/O.
//!
//! `O_DIRECT` tells the kernel to skip its page cache and perform DMA
//! directly between user-space buffers and the block device. This is
//! essential for our use-case because we manage our *own* cache in RAM —
//! double-caching through the kernel would waste memory and add latency.
//!
//! The file descriptor is stored inside `BlockDevice` and closed on `Drop`.

use std::path::{Path, PathBuf};

use crate::error::{CacheError, CacheResult};

use super::device_info::DeviceInfo;

/// A handle to an opened block device using `O_DIRECT`.
///
/// Owns the file descriptor and closes it on drop. All reads and writes
/// through this handle bypass the kernel page cache.
pub struct BlockDevice {
    /// Raw file descriptor returned by `open()`.
    fd: i32,
    /// Path of the device (kept for error messages).
    path: PathBuf,
    /// Cached device metadata.
    info: DeviceInfo,
}

impl BlockDevice {
    /// Open a block device for direct I/O.
    ///
    /// # Arguments
    ///
    /// - `path` — device node (e.g. `/dev/sda`).
    /// - `read_only` — if `true`, opens with `O_RDONLY`; otherwise `O_RDWR`.
    ///
    /// # Errors
    ///
    /// - [`CacheError::DeviceNotFound`] if the path does not exist.
    /// - [`CacheError::NotBlockDevice`] if it is not a block device.
    /// - [`CacheError::PermissionDenied`] if the process lacks privileges.
    /// - [`CacheError::Io`] on any other `open()` failure.
    pub fn open(path: &Path, read_only: bool) -> CacheResult<Self> {
        // Query device info first (validates existence + block device type).
        let info = DeviceInfo::query(path)?;

        let flags = if read_only {
            libc::O_RDONLY | libc::O_DIRECT
        } else {
            libc::O_RDWR | libc::O_DIRECT
        };

        let c_path = std::ffi::CString::new(path.to_string_lossy().as_bytes()).map_err(|_| {
            CacheError::config(format!(
                "device path contains null byte: {}",
                path.display()
            ))
        })?;

        let fd = unsafe { libc::open(c_path.as_ptr(), flags) };
        if fd < 0 {
            return Err(CacheError::io(
                "open (O_DIRECT)",
                std::io::Error::last_os_error(),
            ));
        }

        log::info!(
            "Opened block device {} (fd={}, mode={})",
            path.display(),
            fd,
            if read_only { "read-only" } else { "read-write" },
        );

        Ok(Self {
            fd,
            path: path.to_path_buf(),
            info,
        })
    }

    /// Raw file descriptor for use with `pread` / `pwrite`.
    #[inline]
    pub fn fd(&self) -> i32 {
        self.fd
    }

    /// Reference to the cached device metadata.
    #[inline]
    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    /// Device path.
    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for BlockDevice {
    fn drop(&mut self) {
        if self.fd >= 0 {
            log::debug!(
                "Closing block device {} (fd={})",
                self.path.display(),
                self.fd
            );
            unsafe {
                libc::close(self.fd);
            }
        }
    }
}

// SAFETY: The file descriptor is a plain integer. Sending it across threads
// is safe; concurrent I/O is coordinated at a higher level by the engine.
unsafe impl Send for BlockDevice {}
unsafe impl Sync for BlockDevice {}
