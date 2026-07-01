//! Implements the LRU (Least Recently Used) eviction policy.
//!
//! When the memory pool is full and a new block needs to be cached, the
//! eviction policy selects the best candidate for removal:
//!
//!   1. Prefer **clean** (non-dirty) blocks — they can be evicted immediately
//!      without a disk write.
//!   2. Among clean blocks, evict the one with the **oldest** `last_access`
//!      timestamp (classic LRU).
//!   3. If *all* blocks are dirty, evict the oldest dirty block. The caller
//!      must flush it to disk before reusing the pool slot.

use super::entry::CacheEntry;
use super::lba_table::LbaTable;

/// Result of an eviction decision.
#[derive(Debug)]
pub struct EvictionCandidate {
    /// The LBA of the block that should be evicted.
    pub lba: u64,
    /// The pool index of the evicted block (to be returned to the free-list).
    pub pool_index: usize,
    /// Whether the evicted block is dirty and needs to be flushed first.
    pub needs_flush: bool,
}

/// Select the best eviction candidate from the current cache contents.
///
/// Returns `None` if the table is empty (nothing to evict).
///
/// # Algorithm
///
/// 1. Snapshot all entries sorted by `last_access` ascending (oldest first).
/// 2. Scan for the oldest **clean** entry — if found, return it.
/// 3. If no clean entries exist, return the oldest **dirty** entry and set
///    `needs_flush = true`.
pub fn select_eviction_candidate(table: &LbaTable) -> Option<EvictionCandidate> {
    let entries = table.entries_by_lru(); // sorted oldest → newest

    if entries.is_empty() {
        return None;
    }

    // First pass: find the oldest clean entry.
    if let Some(clean) = find_oldest_clean(&entries) {
        return Some(clean);
    }

    // Second pass: all entries are dirty — evict the oldest dirty one.
    let oldest = &entries[0];
    Some(EvictionCandidate {
        lba: oldest.lba,
        pool_index: oldest.pool_index,
        needs_flush: true,
    })
}

/// Scan sorted entries for the first (oldest) non-dirty entry.
fn find_oldest_clean(entries: &[CacheEntry]) -> Option<EvictionCandidate> {
    entries
        .iter()
        .find(|e| !e.is_dirty())
        .map(|e| EvictionCandidate {
            lba: e.lba,
            pool_index: e.pool_index,
            needs_flush: false,
        })
}

// Unit tests

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn evicts_oldest_clean_first() {
        let table = LbaTable::new(4);

        // Insert three entries with staggered timestamps.
        table.insert(1, CacheEntry::new(1, 10));
        sleep(Duration::from_millis(5));
        table.insert(2, CacheEntry::new(2, 20));
        sleep(Duration::from_millis(5));
        table.insert(3, CacheEntry::new(3, 30));

        // Mark the oldest (LBA 1) as dirty so it should be skipped.
        table.mark_dirty(1);

        let candidate = select_eviction_candidate(&table).unwrap();
        // Should evict LBA 2 (oldest clean), not LBA 1 (dirty).
        assert_eq!(candidate.lba, 2);
        assert!(!candidate.needs_flush);
    }

    #[test]
    fn evicts_oldest_dirty_when_all_dirty() {
        let table = LbaTable::new(4);

        table.insert(1, CacheEntry::new(1, 10));
        sleep(Duration::from_millis(5));
        table.insert(2, CacheEntry::new(2, 20));

        table.mark_dirty(1);
        table.mark_dirty(2);

        let candidate = select_eviction_candidate(&table).unwrap();
        assert_eq!(candidate.lba, 1); // oldest dirty
        assert!(candidate.needs_flush);
    }

    #[test]
    fn empty_table_returns_none() {
        let table = LbaTable::new(4);
        assert!(select_eviction_candidate(&table).is_none());
    }
}
