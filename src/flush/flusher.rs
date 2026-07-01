//! The background write-back thread.
//!
//! Runs in a dedicated OS thread (not async) because:
//!   - `pwrite` with `O_DIRECT` is a blocking syscall on spinning disks.
//!   - A dedicated thread keeps flush latency out of the hot read/write path.
//!
//! Lifecycle:
//!   1. `BackgroundFlusher::spawn()` — launches the thread.
//!   2. The thread loops: sleep → collect dirty entries → pwrite each → repeat.
//!   3. `BackgroundFlusher::shutdown()` — signals the thread, waits for it
//!      to perform one final flush, then joins.

use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::cache::LbaTable;
use crate::error::{CacheError, CacheResult};
use crate::io::writer::{sync_device, write_lba};
use crate::memory::MemoryPool;

use super::scheduler::FlushSchedule;
use super::sync::{flush_channel, FlushNotifier, FlushReceiver, FlushSignal};

/// Manages the background flush thread and provides a notification handle.
pub struct BackgroundFlusher {
    /// Notifier used to wake or stop the flush thread.
    notifier: FlushNotifier,
    /// Join handle for the flush thread. `None` after `shutdown()` completes.
    handle: Option<JoinHandle<()>>,
}

impl BackgroundFlusher {
    /// Spawn the background flush thread.
    ///
    /// # Arguments
    ///
    /// - `fd` — file descriptor of the block device (must be `O_RDWR`).
    /// - `block_size` — size of each cache block in bytes.
    /// - `pool` — shared reference to the memory pool.
    /// - `table` — shared reference to the LBA cache table.
    /// - `schedule` — flush timing configuration.
    pub fn spawn(
        fd: i32,
        block_size: usize,
        pool: Arc<MemoryPool>,
        table: Arc<LbaTable>,
        schedule: FlushSchedule,
    ) -> Self {
        let (notifier, receiver) = flush_channel();

        // Capture log values before moving `schedule` into the closure.
        let log_interval = schedule.interval;
        let log_watermark = schedule.dirty_watermark;

        let handle = thread::Builder::new()
            .name("asm-cache-flusher".into())
            .spawn(move || {
                flush_thread_main(fd, block_size, &pool, &table, &schedule, receiver);
            })
            .expect("failed to spawn flush thread");

        log::info!(
            "Background flusher started (interval={:?}, watermark={:.0}%)",
            log_interval,
            log_watermark * 100.0,
        );

        Self {
            notifier,
            handle: Some(handle),
        }
    }

    /// Get a clone of the notifier (for the engine to trigger immediate
    /// flushes when the dirty watermark is exceeded).
    pub fn notifier(&self) -> FlushNotifier {
        self.notifier.clone()
    }

    /// Gracefully shut down the flush thread.
    ///
    /// Sends a shutdown signal, waits for the thread to perform a final
    /// flush, and then joins it.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::FlushThread`] if the thread panicked.
    pub fn shutdown(&mut self) -> CacheResult<()> {
        self.notifier.shutdown();

        if let Some(handle) = self.handle.take() {
            log::info!("Waiting for flush thread to complete final flush...");
            handle.join().map_err(|_| CacheError::FlushThread {
                reason: "flush thread panicked during shutdown".into(),
            })?;
            log::info!("Flush thread terminated cleanly.");
        }

        Ok(())
    }
}

impl Drop for BackgroundFlusher {
    fn drop(&mut self) {
        // Best-effort shutdown if the caller forgot to call `shutdown()`.
        if self.handle.is_some() {
            log::warn!("BackgroundFlusher dropped without explicit shutdown — forcing stop");
            let _ = self.shutdown();
        }
    }
}

// Flush thread entry point

/// Main loop of the flush thread.
fn flush_thread_main(
    fd: i32,
    block_size: usize,
    pool: &MemoryPool,
    table: &LbaTable,
    schedule: &FlushSchedule,
    receiver: FlushReceiver,
) {
    log::debug!("Flush thread started.");

    loop {
        // Wait for the next signal or timeout
        let signal = receiver.wait(schedule.interval);

        // Perform a flush cycle
        let flushed = flush_dirty_blocks(fd, block_size, pool, table);
        if flushed > 0 {
            log::debug!("Flushed {flushed} dirty block(s) to disk.");

            // Issue a hardware sync after each cycle to commit writes.
            if let Err(e) = sync_device(fd) {
                log::error!("fsync failed after flush: {e}");
            }
        }

        // Check for shutdown
        if signal == Some(FlushSignal::Shutdown) || receiver.is_stopped() {
            // Final flush already done above — exit the loop.
            log::info!("Flush thread received shutdown signal.");
            break;
        }
    }

    log::debug!("Flush thread exited.");
}

/// Iterate over all dirty entries, write each back to the device, and
/// mark it clean. Returns the number of blocks flushed.
fn flush_dirty_blocks(fd: i32, block_size: usize, pool: &MemoryPool, table: &LbaTable) -> usize {
    let dirty = table.dirty_entries();

    if dirty.is_empty() {
        return 0;
    }

    let mut flushed: usize = 0;

    for (lba, pool_index) in &dirty {
        let block = pool.get_block(*pool_index);
        let data = block.as_ref();

        match write_lba(fd, *lba, block_size, data) {
            Ok(()) => {
                table.mark_clean(*lba);
                flushed += 1;
            }
            Err(e) => {
                log::error!("Failed to flush LBA {lba} to disk: {e}");
                // Continue flushing other blocks — one failure should not
                // abort the entire cycle.
            }
        }
    }

    flushed
}
