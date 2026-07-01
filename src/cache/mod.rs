//! Module facade — exposes the cache entry, LBA table, and eviction policy.

pub mod entry;
pub mod eviction;
pub mod lba_table;

pub use entry::CacheEntry;
pub use eviction::{select_eviction_candidate, EvictionCandidate};
pub use lba_table::{LbaTable, TableStats};
