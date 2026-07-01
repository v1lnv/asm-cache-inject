//! A fixed-capacity memory pool that pre-allocates a single large contiguous
//! page-aligned region and sub-divides it into equally sized blocks.
//!
//! The pool uses a free-list for O(1) allocation and deallocation. All blocks
//! share the alignment of the backing buffer so they are safe for O_DIRECT I/O.
//!
//! Thread safety is provided by a `parking_lot::Mutex` around the free-list.
//! The actual block memory is **not** behind a lock — only the bookkeeping is.

use parking_lot::Mutex;

use crate::error::{CacheError, CacheResult};

use super::aligned_allocator::{AlignedBuffer, PAGE_ALIGNMENT};
use super::block_buffer::BlockBuffer;

/// Pre-allocated pool of page-aligned cache blocks.
///
/// # Capacity
///
/// Given a `total_size` of 256 MiB and a `block_size` of 4096 bytes the pool
/// holds 65 536 blocks (256 × 1024 × 1024 / 4096).
pub struct MemoryPool {
    /// The single contiguous backing allocation.
    _backing: AlignedBuffer,
    /// Raw base pointer (cached from `_backing` for fast offset arithmetic).
    base_ptr: *mut u8,
    /// Size of each individual block in bytes.
    block_size: usize,
    /// Total number of blocks carved out of the backing region.
    capacity: usize,
    /// Free-list: indices of blocks that are currently unoccupied.
    free_list: Mutex<Vec<usize>>,
}

// SAFETY: The backing buffer is Send + Sync and the free-list is behind a
// Mutex. Raw pointer `base_ptr` never changes after construction.
unsafe impl Send for MemoryPool {}
unsafe impl Sync for MemoryPool {}

impl MemoryPool {
    /// Create a new pool of `total_size` bytes divided into blocks of
    /// `block_size` bytes each.
    ///
    /// # Constraints
    ///
    /// - `block_size` must be a multiple of [`PAGE_ALIGNMENT`] (4096).
    /// - `total_size` must be a multiple of `block_size`.
    /// - Both values must be non-zero.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::Config`] on invalid parameters or
    /// [`CacheError::Allocation`] if the backing allocation fails.
    pub fn new(total_size: usize, block_size: usize) -> CacheResult<Self> {
        // Validate parameters
        if block_size == 0 || !block_size.is_multiple_of(PAGE_ALIGNMENT) {
            return Err(CacheError::config(format!(
                "block_size ({block_size}) must be a non-zero multiple of {PAGE_ALIGNMENT}"
            )));
        }
        if total_size == 0 || !total_size.is_multiple_of(block_size) {
            return Err(CacheError::config(format!(
                "total_size ({total_size}) must be a non-zero multiple of block_size ({block_size})"
            )));
        }

        let capacity = total_size / block_size;

        // Allocate contiguous backing buffer
        let mut backing = AlignedBuffer::new(total_size)?;
        let base_ptr = backing.as_mut_ptr();

        // Build the free-list (all blocks start as free)
        let free_list: Vec<usize> = (0..capacity).collect();

        log::info!(
            "Memory pool initialised: {} blocks × {} B = {} MiB",
            capacity,
            block_size,
            total_size / (1024 * 1024),
        );

        Ok(Self {
            _backing: backing,
            base_ptr,
            block_size,
            capacity,
            free_list: Mutex::new(free_list),
        })
    }

    /// Allocate one block from the pool.
    ///
    /// Returns the **pool index** of the allocated block together with a
    /// [`BlockBuffer`] handle for reading/writing its contents.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::PoolExhausted`] when all blocks are in use.
    pub fn allocate(&self) -> CacheResult<(usize, BlockBuffer)> {
        let mut free = self.free_list.lock();

        let index = free.pop().ok_or(CacheError::PoolExhausted {
            in_use: self.capacity,
            capacity: self.capacity,
        })?;

        let buf = self.buffer_for_index(index);
        Ok((index, buf))
    }

    /// Return a previously allocated block to the pool.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if `index >= capacity`.
    pub fn deallocate(&self, index: usize) {
        debug_assert!(
            index < self.capacity,
            "deallocate index {index} out of range (capacity {})",
            self.capacity,
        );
        self.free_list.lock().push(index);
    }

    /// Obtain a [`BlockBuffer`] handle for the block at `index` **without**
    /// changing the free-list.
    ///
    /// This is used to re-access a block that is already allocated (e.g. on a
    /// cache hit). The caller must ensure the block is currently allocated.
    pub fn get_block(&self, index: usize) -> BlockBuffer {
        debug_assert!(
            index < self.capacity,
            "get_block index {index} out of range (capacity {})",
            self.capacity,
        );
        self.buffer_for_index(index)
    }

    /// Number of blocks currently available for allocation.
    pub fn free_count(&self) -> usize {
        self.free_list.lock().len()
    }

    /// Number of blocks currently in use.
    pub fn used_count(&self) -> usize {
        self.capacity - self.free_count()
    }

    /// Total block capacity of the pool.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Size of a single block in bytes.
    #[inline]
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Base pointer for the backing region (used by ASM routines that need
    /// the raw address for offset calculations).
    #[inline]
    pub fn base_ptr(&self) -> *const u8 {
        self.base_ptr
    }

    // Internal helpers

    /// Compute the pointer for block `index` and wrap it in a `BlockBuffer`.
    fn buffer_for_index(&self, index: usize) -> BlockBuffer {
        let offset = index * self.block_size;
        // SAFETY: `offset` is within the backing allocation (enforced by the
        // `debug_assert` at the call-site) and the backing buffer is alive
        // for the lifetime of the pool.
        unsafe {
            let ptr = self.base_ptr.add(offset);
            BlockBuffer::from_raw(ptr, self.block_size)
        }
    }
}

// Unit tests

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK_SIZE: usize = 4096;
    const POOL_SIZE: usize = BLOCK_SIZE * 16; // 16 blocks

    #[test]
    fn basic_allocate_and_deallocate() {
        let pool = MemoryPool::new(POOL_SIZE, BLOCK_SIZE).unwrap();
        assert_eq!(pool.capacity(), 16);
        assert_eq!(pool.free_count(), 16);

        let (idx, _buf) = pool.allocate().unwrap();
        assert_eq!(pool.free_count(), 15);

        pool.deallocate(idx);
        assert_eq!(pool.free_count(), 16);
    }

    #[test]
    fn exhaust_pool() {
        let pool = MemoryPool::new(POOL_SIZE, BLOCK_SIZE).unwrap();
        let mut indices = Vec::new();

        for _ in 0..16 {
            let (idx, _) = pool.allocate().unwrap();
            indices.push(idx);
        }

        // 17th allocation should fail.
        assert!(pool.allocate().is_err());

        // Free one and allocate again.
        pool.deallocate(indices.pop().unwrap());
        assert!(pool.allocate().is_ok());
    }

    #[test]
    fn block_buffers_are_page_aligned() {
        let pool = MemoryPool::new(POOL_SIZE, BLOCK_SIZE).unwrap();
        for _ in 0..16 {
            let (_, buf) = pool.allocate().unwrap();
            assert_eq!(buf.as_ptr() as usize % PAGE_ALIGNMENT, 0);
        }
    }

    #[test]
    fn rejects_bad_parameters() {
        assert!(MemoryPool::new(0, BLOCK_SIZE).is_err());
        assert!(MemoryPool::new(POOL_SIZE, 0).is_err());
        assert!(MemoryPool::new(POOL_SIZE, 1000).is_err()); // not aligned
        assert!(MemoryPool::new(5000, BLOCK_SIZE).is_err()); // not multiple
    }
}
