//! The central orchestrator that ties together the memory pool, cache table,
//! block device, ASM layer, and background flusher into a coherent cache
//! engine with a simple read/write API.
//!
//! Thread model:
//!   - The engine itself is `Send + Sync` (wrapped in an `Arc` by the CLI).
//!   - One dedicated background thread runs the flusher.
//!   - Read/write operations are serialised at the LBA-table level via
//!     `RwLock` (reads are concurrent; writes are exclusive per-LBA).

use std::sync::Arc;

use crate::asm;
use crate::cache::LbaTable;
use crate::error::CacheResult;
use crate::flush::{BackgroundFlusher, FlushSchedule};
use crate::io::BlockDevice;
use crate::memory::MemoryPool;

use super::config::CacheConfig;
use super::read_path::{cached_read, ReadResult};
use super::write_path::{cached_write, WriteResult};

/// The main cache engine.
///
/// Constructed from a [`CacheConfig`], manages the full lifecycle of the
/// cache: initialisation → operation → shutdown.
pub struct CacheEngine {
    /// Configuration snapshot.
    config: CacheConfig,
    /// Opened block device handle.
    device: BlockDevice,
    /// Pre-allocated page-aligned memory pool.
    pool: Arc<MemoryPool>,
    /// LBA → pool-index cache table.
    table: Arc<LbaTable>,
    /// Background write-back thread (started on [`start()`]).
    flusher: Option<BackgroundFlusher>,
    /// Flush schedule configuration.
    flush_schedule: FlushSchedule,
}

impl CacheEngine {
    /// Create a new engine from the given configuration.
    ///
    /// This allocates the memory pool, opens the device, and prepares the
    /// cache table, but does **not** start the background flusher yet.
    /// Call [`start()`](CacheEngine::start) to begin flushing.
    ///
    /// # Errors
    ///
    /// Returns an error if configuration validation fails, memory allocation
    /// fails, or the device cannot be opened.
    pub fn new(config: CacheConfig) -> CacheResult<Self> {
        config.validate()?;

        // Log CPU capabilities before starting.
        asm::log_cpu_capabilities();

        log::info!("Initialising cache engine:\n{config}");

        // Allocate memory pool
        let pool = Arc::new(MemoryPool::new(
            config.cache_size_bytes(),
            config.block_size,
        )?);

        // Open block device
        let device = BlockDevice::open(&config.device_path, config.read_only)?;

        // Create cache table
        let table = Arc::new(LbaTable::new(config.block_count()));

        // Prepare flush schedule
        let flush_schedule = FlushSchedule::new(config.flush_interval_secs, config.dirty_watermark);

        Ok(Self {
            config,
            device,
            pool,
            table,
            flusher: None,
            flush_schedule,
        })
    }

    /// Start the background flusher thread.
    ///
    /// Must be called after [`new()`](CacheEngine::new) and before any
    /// write operations. Read-only usage works without starting the flusher.
    pub fn start(&mut self) {
        if self.flusher.is_some() {
            log::warn!("Flusher already running — ignoring duplicate start()");
            return;
        }

        if self.config.read_only {
            log::info!("Read-only mode — background flusher not started.");
            return;
        }

        let flusher = BackgroundFlusher::spawn(
            self.device.fd(),
            self.config.block_size,
            Arc::clone(&self.pool),
            Arc::clone(&self.table),
            self.flush_schedule.clone(),
        );

        self.flusher = Some(flusher);
    }

    /// Gracefully shut down the engine.
    ///
    /// 1. Signals the flusher to perform a final write-back.
    /// 2. Waits for the flusher thread to join.
    /// 3. The block device is closed on drop.
    pub fn stop(&mut self) -> CacheResult<()> {
        log::info!("Shutting down cache engine...");

        if let Some(ref mut flusher) = self.flusher {
            flusher.shutdown()?;
        }
        self.flusher = None;

        let stats = self.table.stats();
        log::info!("Final cache stats: {stats}");

        Ok(())
    }

    // Public read/write API

    /// Read a single block at `lba` into `buf`.
    ///
    /// `buf` must be exactly `block_size` bytes long.
    pub fn read(&self, lba: u64, buf: &mut [u8]) -> CacheResult<ReadResult> {
        self.validate_lba(lba)?;

        let notifier = self.flush_notifier_or_noop();
        cached_read(lba, buf, &self.device, &self.pool, &self.table, &notifier)
    }

    /// Write a single block at `lba` from `buf`.
    ///
    /// `buf` must be exactly `block_size` bytes long.
    pub fn write(&self, lba: u64, buf: &[u8]) -> CacheResult<WriteResult> {
        self.validate_lba(lba)?;

        let notifier = self.flush_notifier_or_noop();
        cached_write(
            lba,
            buf,
            &self.device,
            &self.pool,
            &self.table,
            &notifier,
            &self.flush_schedule,
            self.config.read_only,
        )
    }

    // Accessors

    /// Reference to the engine configuration.
    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    /// Reference to the block device handle.
    pub fn device(&self) -> &BlockDevice {
        &self.device
    }

    /// Reference to the memory pool.
    pub fn pool(&self) -> &Arc<MemoryPool> {
        &self.pool
    }

    /// Reference to the LBA table.
    pub fn table(&self) -> &Arc<LbaTable> {
        &self.table
    }

    /// Block size in bytes.
    pub fn block_size(&self) -> usize {
        self.config.block_size
    }

    // Internal helpers

    /// Validate that an LBA is within the device's addressable range.
    fn validate_lba(&self, lba: u64) -> CacheResult<()> {
        let max_lba = self.device.info().max_lba(self.config.block_size as u64);
        if lba >= max_lba {
            return Err(crate::error::CacheError::LbaOutOfRange { lba, max_lba });
        }
        Ok(())
    }

    /// Get the flusher's notifier, or a no-op stand-in if no flusher is
    /// running (read-only mode or before `start()`).
    fn flush_notifier_or_noop(&self) -> crate::flush::FlushNotifier {
        if let Some(ref flusher) = self.flusher {
            flusher.notifier()
        } else {
            // Create a dummy notifier whose sends go nowhere.
            let (notifier, _receiver) = crate::flush::sync::flush_channel();
            notifier
        }
    }
}
