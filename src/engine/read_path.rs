//! Handles read requests through the cache.
//!
//! Flow:
//!   1. Check LBA table for a cache hit.
//!   2. HIT  → copy data from pool block to user buffer (temporal copy).
//!   3. MISS → allocate pool block, pread from device, store in pool, insert into table, copy to user buffer.
//!   4. If pool is full → trigger LRU eviction first.

use std::sync::Arc;

use crate::asm;
use crate::cache::{select_eviction_candidate, CacheEntry, LbaTable};
use crate::error::{CacheError, CacheResult};
use crate::flush::FlushNotifier;
use crate::io::{read_lba, write_lba, BlockDevice};
use crate::memory::MemoryPool;

/// Statistics returned by a single read operation.
#[derive(Debug, Clone, Copy)]
pub struct ReadResult {
    /// Whether the read was served from the cache.
    pub cache_hit: bool,
}

/// Execute a cached read for a single LBA.
///
/// `user_buf` receives the block data (must be `block_size` bytes).
pub fn cached_read(
    lba: u64,
    user_buf: &mut [u8],
    device: &BlockDevice,
    pool: &Arc<MemoryPool>,
    table: &Arc<LbaTable>,
    flush_notifier: &FlushNotifier,
) -> CacheResult<ReadResult> {
    let block_size = pool.block_size();
    debug_assert_eq!(user_buf.len(), block_size);

    // 1. Cache hit check
    if let Some(pool_index) = table.get_and_touch(lba) {
        // HIT: copy from pool → user buffer using temporal (cache-warm) copy.
        let block = pool.get_block(pool_index);
        unsafe {
            asm::fast_copy_temporal(user_buf.as_mut_ptr(), block.as_ptr(), block_size);
        }
        return Ok(ReadResult { cache_hit: true });
    }

    // 2. Cache miss — need to fetch from device

    // Ensure we have a free pool slot (evict if necessary).
    let (pool_index, mut pool_block) = allocate_or_evict(pool, table, device, flush_notifier)?;

    // Read from device into the pool block.
    read_lba(device.fd(), lba, block_size, pool_block.as_mut())?;

    // Insert into the LBA table.
    let entry = CacheEntry::new(lba, pool_index);
    table.insert(lba, entry);

    // Copy from pool → user buffer.
    unsafe {
        asm::fast_copy_temporal(user_buf.as_mut_ptr(), pool_block.as_ptr(), block_size);
    }

    Ok(ReadResult { cache_hit: false })
}

/// Try to allocate a pool block. If the pool is exhausted, evict the LRU
/// entry first.
pub(crate) fn allocate_or_evict(
    pool: &Arc<MemoryPool>,
    table: &Arc<LbaTable>,
    device: &BlockDevice,
    flush_notifier: &FlushNotifier,
) -> CacheResult<(usize, crate::memory::BlockBuffer)> {
    // Fast path: free slot available.
    if let Ok(result) = pool.allocate() {
        return Ok(result);
    }

    // Slow path: evict the LRU entry.
    let candidate = select_eviction_candidate(table).ok_or_else(|| {
        CacheError::internal("pool exhausted but cache table is empty — inconsistent state")
    })?;

    // If the eviction candidate is dirty, flush it to disk first.
    if candidate.needs_flush {
        let block = pool.get_block(candidate.pool_index);
        write_lba(
            device.fd(),
            candidate.lba,
            pool.block_size(),
            block.as_ref(),
        )?;
        log::debug!("Eviction: flushed dirty LBA {} before reuse", candidate.lba);

        // Wake the flusher so it can handle any remaining dirty blocks
        // while we continue.
        flush_notifier.wake();
    }

    // Remove the evicted entry from the table and return the slot.
    table.remove(candidate.lba);
    pool.deallocate(candidate.pool_index);

    // Now allocate — this should succeed since we just freed a slot.
    pool.allocate()
}
