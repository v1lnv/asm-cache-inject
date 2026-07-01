//! A thread-safe hash map that maps Logical Block Addresses (LBAs) from the
//! block device to [`CacheEntry`] metadata in the memory pool.
//!
//! This is the central data structure that decides whether a request is a
//! "cache hit" or "cache miss". It is protected by a `parking_lot::RwLock`
//! so that:
//!   - Multiple reader threads can perform lookups concurrently.
//!   - A single writer can insert / remove / update entries exclusively.

use std::collections::HashMap;

use parking_lot::RwLock;

use super::entry::CacheEntry;

/// Thread-safe LBA → CacheEntry lookup table.
///
/// Internally backed by a `HashMap<u64, CacheEntry>` behind a `RwLock`.
/// The table never allocates pool memory itself — it only stores metadata.
pub struct LbaTable {
    inner: RwLock<HashMap<u64, CacheEntry>>,
}

impl LbaTable {
    /// Create an empty table with a pre-allocated capacity hint.
    ///
    /// `capacity` should match the memory pool's block count so that the
    /// hash map does not need to reallocate during normal operation.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: RwLock::new(HashMap::with_capacity(capacity)),
        }
    }

    // Lookups (read-locked)

    /// Check whether `lba` is cached. Returns `true` on cache hit.
    pub fn contains(&self, lba: u64) -> bool {
        self.inner.read().contains_key(&lba)
    }

    /// Look up `lba` and return a **clone** of its entry if present.
    ///
    /// A clone is returned (rather than a reference) because the `RwLock`
    /// guard cannot outlive this function. The clone is cheap since
    /// `CacheEntry` is small (< 64 bytes).
    pub fn get(&self, lba: u64) -> Option<CacheEntry> {
        self.inner.read().get(&lba).cloned()
    }

    /// Look up `lba`, touch the entry (update access time), and return the
    /// pool index. This is the hot path for cache-hit reads.
    pub fn get_and_touch(&self, lba: u64) -> Option<usize> {
        let mut map = self.inner.write();
        if let Some(entry) = map.get_mut(&lba) {
            entry.touch();
            Some(entry.pool_index)
        } else {
            None
        }
    }

    /// Return the number of cached entries.
    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    /// Returns `true` if the table is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.read().is_empty()
    }

    // Mutations (write-locked)

    /// Insert a new entry. If an entry for `lba` already exists it is
    /// replaced and the old entry is returned.
    pub fn insert(&self, lba: u64, entry: CacheEntry) -> Option<CacheEntry> {
        self.inner.write().insert(lba, entry)
    }

    /// Remove the entry for `lba` and return it, or `None` if it was not
    /// present.
    pub fn remove(&self, lba: u64) -> Option<CacheEntry> {
        self.inner.write().remove(&lba)
    }

    /// Mark the entry for `lba` as dirty. No-op if `lba` is not cached.
    pub fn mark_dirty(&self, lba: u64) {
        if let Some(entry) = self.inner.write().get_mut(&lba) {
            entry.mark_dirty();
        }
    }

    /// Mark the entry for `lba` as clean (after a successful flush).
    /// No-op if `lba` is not cached.
    pub fn mark_clean(&self, lba: u64) {
        if let Some(entry) = self.inner.write().get_mut(&lba) {
            entry.mark_clean();
        }
    }

    // Bulk queries

    /// Collect all dirty entries as `(lba, pool_index)` pairs.
    ///
    /// Used by the background flusher to decide which blocks need to be
    /// written back to disk.
    pub fn dirty_entries(&self) -> Vec<(u64, usize)> {
        self.inner
            .read()
            .values()
            .filter(|e| e.is_dirty())
            .map(|e| (e.lba, e.pool_index))
            .collect()
    }

    /// Return a snapshot of **all** entries sorted by `last_access` ascending
    /// (oldest first). Used by the eviction policy.
    pub fn entries_by_lru(&self) -> Vec<CacheEntry> {
        let map = self.inner.read();
        let mut entries: Vec<CacheEntry> = map.values().cloned().collect();
        entries.sort_by_key(|e| e.last_access);
        entries
    }

    /// Return aggregate statistics for diagnostics.
    pub fn stats(&self) -> TableStats {
        let map = self.inner.read();
        let total = map.len();
        let dirty = map.values().filter(|e| e.is_dirty()).count();
        let total_hits: u64 = map.values().map(|e| e.access_count).sum();
        TableStats {
            total_entries: total,
            dirty_entries: dirty,
            total_hits,
        }
    }
}

/// Diagnostic statistics snapshot from the LBA table.
#[derive(Debug, Clone)]
pub struct TableStats {
    /// Total number of entries in the table.
    pub total_entries: usize,
    /// Number of entries marked dirty (pending flush).
    pub dirty_entries: usize,
    /// Sum of all access counts across all entries.
    pub total_hits: u64,
}

impl std::fmt::Display for TableStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "entries={}, dirty={}, total_hits={}",
            self.total_entries, self.dirty_entries, self.total_hits,
        )
    }
}

// Unit tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_lookup() {
        let table = LbaTable::new(16);
        let entry = CacheEntry::new(42, 7);
        assert!(table.insert(42, entry).is_none());
        assert!(table.contains(42));
        assert!(!table.contains(99));

        let got = table.get(42).unwrap();
        assert_eq!(got.lba, 42);
        assert_eq!(got.pool_index, 7);
    }

    #[test]
    fn dirty_tracking() {
        let table = LbaTable::new(16);
        table.insert(10, CacheEntry::new(10, 0));

        assert!(table.dirty_entries().is_empty());

        table.mark_dirty(10);
        let dirty = table.dirty_entries();
        assert_eq!(dirty.len(), 1);
        assert_eq!(dirty[0], (10, 0));

        table.mark_clean(10);
        assert!(table.dirty_entries().is_empty());
    }

    #[test]
    fn remove_entry() {
        let table = LbaTable::new(16);
        table.insert(5, CacheEntry::new(5, 3));
        assert!(table.remove(5).is_some());
        assert!(table.remove(5).is_none());
        assert!(!table.contains(5));
    }

    #[test]
    fn get_and_touch_increments_access() {
        let table = LbaTable::new(16);
        table.insert(1, CacheEntry::new(1, 0));

        // Initial access_count is 1.
        let pool_idx = table.get_and_touch(1).unwrap();
        assert_eq!(pool_idx, 0);

        // After touch, access_count should be 2.
        let entry = table.get(1).unwrap();
        assert_eq!(entry.access_count, 2);
    }
}
