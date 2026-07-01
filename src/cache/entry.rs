//! Represents a single cached block in the LBA → RAM mapping table.
//!
//! Each entry tracks:
//!   - Which LBA it corresponds to.
//!   - Where in the memory pool its data lives (pool index).
//!   - Whether the cached copy has been modified (dirty flag).
//!   - Access timestamps for LRU eviction.

use std::time::Instant;

/// Metadata for one cached block.
///
/// The actual block *data* lives in the memory pool — this struct only holds
/// the bookkeeping needed for cache management (lookup, eviction, flush).
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Logical Block Address on the source device.
    pub lba: u64,

    /// Index of the block within the [`crate::memory::MemoryPool`].
    /// Used to retrieve the data pointer without an extra hash lookup.
    pub pool_index: usize,

    /// `true` if the cached data has been written to but not yet flushed
    /// back to the block device. The background flusher uses this flag to
    /// decide which blocks need write-back.
    pub dirty: bool,

    /// Timestamp of the most recent read or write access. Updated on every
    /// cache hit and used by the LRU eviction policy.
    pub last_access: Instant,

    /// Running count of accesses (hits). Useful for diagnostics and for
    /// more sophisticated eviction policies (e.g. LFU hybrid).
    pub access_count: u64,
}

impl CacheEntry {
    /// Create a new entry for `lba` backed by the pool block at `pool_index`.
    ///
    /// The entry starts as **clean** (not dirty) with an access count of 1
    /// (the initial load counts as the first access).
    pub fn new(lba: u64, pool_index: usize) -> Self {
        Self {
            lba,
            pool_index,
            dirty: false,
            last_access: Instant::now(),
            access_count: 1,
        }
    }

    /// Record a new access (cache hit). Updates `last_access` and increments
    /// the hit counter.
    #[inline]
    pub fn touch(&mut self) {
        self.last_access = Instant::now();
        self.access_count = self.access_count.saturating_add(1);
    }

    /// Mark this entry as dirty (pending write-back).
    #[inline]
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.touch();
    }

    /// Clear the dirty flag after a successful flush to disk.
    #[inline]
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Returns `true` if this block needs to be flushed to disk.
    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}
