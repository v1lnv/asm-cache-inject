//! Handles write requests through the cache using a **write-back** strategy.
//!
//! Flow:
//!   1. Check LBA table for an existing entry.
//!   2. EXISTS → overwrite pool block with user data (non-temporal copy) and mark dirty.
//!   3. NEW    → allocate pool block, write user data (non-temporal copy), insert into table, and mark dirty.
//!   4. If dirty watermark exceeded → send immediate wake to flusher.
//!
//! The background flusher is responsible for asynchronously writing dirty
//! blocks back to the device. This write path never touches the device
//! directly — all writes go to RAM first.

use std::sync::Arc;

use crate::asm;
use crate::cache::{CacheEntry, LbaTable};
use crate::error::{CacheError, CacheResult};
use crate::flush::{FlushNotifier, FlushSchedule};
use crate::io::BlockDevice;
use crate::memory::MemoryPool;

use super::read_path::allocate_or_evict;

/// Statistics returned by a single write operation.
#[derive(Debug, Clone, Copy)]
pub struct WriteResult {
    /// Whether the write updated an existing cached block (`true`) or
    /// allocated a new one (`false`).
    pub was_update: bool,
}

/// Execute a cached write for a single LBA.
///
/// `user_buf` contains the data to write (must be `block_size` bytes).
///
/// # Errors
///
/// Returns [`CacheError::Config`] if the engine is in read-only mode.
#[allow(clippy::too_many_arguments)]
pub fn cached_write(
    lba: u64,
    user_buf: &[u8],
    device: &BlockDevice,
    pool: &Arc<MemoryPool>,
    table: &Arc<LbaTable>,
    flush_notifier: &FlushNotifier,
    schedule: &FlushSchedule,
    read_only: bool,
) -> CacheResult<WriteResult> {
    if read_only {
        return Err(CacheError::config(
            "write rejected: engine is in read-only mode",
        ));
    }

    let block_size = pool.block_size();
    debug_assert_eq!(user_buf.len(), block_size);

    // 1. Check for existing entry
    if let Some(entry) = table.get(lba) {
        // UPDATE: overwrite the existing pool block with non-temporal copy.
        let mut block = pool.get_block(entry.pool_index);
        unsafe {
            asm::fast_copy_nontemporal(block.as_mut_ptr(), user_buf.as_ptr(), block_size);
        }
        table.mark_dirty(lba);

        maybe_wake_flusher(table, flush_notifier, schedule);

        return Ok(WriteResult { was_update: true });
    }

    // 2. New entry — allocate a pool block
    let (pool_index, mut pool_block) = allocate_or_evict(pool, table, device, flush_notifier)?;

    // Write user data into the pool block using non-temporal stores.
    unsafe {
        asm::fast_copy_nontemporal(pool_block.as_mut_ptr(), user_buf.as_ptr(), block_size);
    }

    // Insert into the table and mark dirty.
    let mut entry = CacheEntry::new(lba, pool_index);
    entry.mark_dirty();
    table.insert(lba, entry);

    maybe_wake_flusher(table, flush_notifier, schedule);

    Ok(WriteResult { was_update: false })
}

/// If the dirty ratio exceeds the watermark, send an immediate wake signal
/// to the background flusher.
fn maybe_wake_flusher(table: &LbaTable, notifier: &FlushNotifier, schedule: &FlushSchedule) {
    let stats = table.stats();
    if schedule.should_flush_immediately(stats.dirty_entries, stats.total_entries) {
        log::debug!(
            "Dirty watermark exceeded ({}/{}), waking flusher",
            stats.dirty_entries,
            stats.total_entries,
        );
        notifier.wake();
    }
}
