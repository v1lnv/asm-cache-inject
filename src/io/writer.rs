//! Direct block writes to a block device using `pwrite()`.
//!
//! Like `pread`, `pwrite` is offset-atomic and does not modify the file
//! position, making it safe for concurrent use from the background flush
//! thread.
//!
//! Writes through an `O_DIRECT` descriptor bypass the kernel page cache and
//! go directly to the device's DMA controller.

use crate::error::{CacheError, CacheResult};

/// Write exactly `buf.len()` bytes to the block device at byte offset
/// `offset`.
///
/// # Arguments
///
/// - `fd` — file descriptor opened with `O_DIRECT | O_RDWR`.
/// - `offset` — byte offset into the device (must be sector-aligned).
/// - `buf` — source buffer (must be page-aligned and its length must be a
///   multiple of the sector size).
///
/// # Errors
///
/// Returns [`CacheError::Io`] if `pwrite` fails or produces a short write.
pub fn write_block(fd: i32, offset: u64, buf: &[u8]) -> CacheResult<()> {
    debug_assert_eq!(
        buf.as_ptr() as usize % 4096,
        0,
        "buffer must be page-aligned for O_DIRECT"
    );
    debug_assert_eq!(offset % 512, 0, "offset must be sector-aligned");

    let len = buf.len();
    let mut total_written: usize = 0;

    while total_written < len {
        let ret = unsafe {
            libc::pwrite(
                fd,
                buf.as_ptr().add(total_written) as *const libc::c_void,
                len - total_written,
                (offset + total_written as u64) as libc::off_t,
            )
        };

        if ret < 0 {
            return Err(CacheError::io("pwrite", std::io::Error::last_os_error()));
        }
        if ret == 0 {
            return Err(CacheError::Io {
                operation: "pwrite",
                source: std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    format!(
                        "zero-length write at offset {} (wrote {total_written} of {len} bytes)",
                        offset + total_written as u64,
                    ),
                ),
            });
        }

        total_written += ret as usize;
    }

    Ok(())
}

/// Convenience wrapper: write a single LBA-addressed block.
///
/// Computes the byte offset as `lba * block_size` and delegates to
/// [`write_block`].
pub fn write_lba(fd: i32, lba: u64, block_size: usize, buf: &[u8]) -> CacheResult<()> {
    debug_assert_eq!(buf.len(), block_size, "buffer length must match block_size");
    let offset = lba * block_size as u64;
    write_block(fd, offset, buf)
}

/// Issue a hardware flush / sync (`fsync`) on the file descriptor.
///
/// This ensures that all previously written data has been committed to the
/// physical media. Called at the end of each flush cycle.
pub fn sync_device(fd: i32) -> CacheResult<()> {
    let ret = unsafe { libc::fsync(fd) };
    if ret < 0 {
        return Err(CacheError::io("fsync", std::io::Error::last_os_error()));
    }
    Ok(())
}
