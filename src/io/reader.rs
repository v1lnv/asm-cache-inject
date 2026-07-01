//! Direct block reads from a block device using `pread()`.
//!
//! `pread` reads from a file descriptor at a given offset without changing
//! the file position, which makes it safe for concurrent use from multiple
//! threads (each call is atomic w.r.t. the offset).
//!
//! The caller must provide a page-aligned buffer (enforced by the type
//! system via `AlignedBuffer` or `BlockBuffer`).

use crate::error::{CacheError, CacheResult};

/// Read exactly `buf.len()` bytes from the block device at byte offset
/// `offset` into `buf`.
///
/// # Arguments
///
/// - `fd` — file descriptor opened with `O_DIRECT`.
/// - `offset` — byte offset into the device (must be sector-aligned).
/// - `buf` — destination buffer (must be page-aligned and its length must
///   be a multiple of the sector size).
///
/// # Errors
///
/// Returns [`CacheError::Io`] if `pread` fails or returns a short read.
pub fn read_block(fd: i32, offset: u64, buf: &mut [u8]) -> CacheResult<()> {
    debug_assert_eq!(
        buf.as_ptr() as usize % 4096,
        0,
        "buffer must be page-aligned for O_DIRECT"
    );
    debug_assert_eq!(offset % 512, 0, "offset must be sector-aligned");

    let len = buf.len();
    let mut total_read: usize = 0;

    // Loop to handle potential short reads (rare with block devices, but
    // defensive programming is free).
    while total_read < len {
        let ret = unsafe {
            libc::pread(
                fd,
                buf.as_mut_ptr().add(total_read) as *mut libc::c_void,
                len - total_read,
                (offset + total_read as u64) as libc::off_t,
            )
        };

        if ret < 0 {
            return Err(CacheError::io("pread", std::io::Error::last_os_error()));
        }
        if ret == 0 {
            return Err(CacheError::Io {
                operation: "pread",
                source: std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    format!(
                        "unexpected EOF at offset {} (read {total_read} of {len} bytes)",
                        offset + total_read as u64,
                    ),
                ),
            });
        }

        total_read += ret as usize;
    }

    Ok(())
}

/// Convenience wrapper: read a single LBA-addressed block.
///
/// Computes the byte offset as `lba * block_size` and delegates to
/// [`read_block`].
pub fn read_lba(fd: i32, lba: u64, block_size: usize, buf: &mut [u8]) -> CacheResult<()> {
    debug_assert_eq!(buf.len(), block_size, "buffer length must match block_size");
    let offset = lba * block_size as u64;
    read_block(fd, offset, buf)
}
