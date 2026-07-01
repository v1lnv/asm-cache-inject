//! Thread synchronisation primitives for coordinating the background flush
//! thread with the main engine.
//!
//! The flush thread runs in a loop:
//!   1. Sleep for the configured interval (or until woken by a notification).
//!   2. Collect dirty entries from the LBA table.
//!   3. Write them back to the device.
//!   4. Repeat.
//!
//! Shutdown is signalled by setting an `AtomicBool` stop flag and
//! notifying the condvar so the thread wakes up immediately rather than
//! waiting for the next interval to expire.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam::channel::{self, Receiver, Sender};

/// A notification channel used to signal the flush thread.
///
/// Supports two operations:
/// - **Wake**: Tell the flush thread to run a cycle immediately.
/// - **Stop**: Tell the flush thread to perform a final flush and exit.
#[derive(Clone)]
pub struct FlushNotifier {
    /// Sending half — held by the engine / signal handler.
    tx: Sender<FlushSignal>,
    /// Atomic stop flag — set once and never cleared.
    stop: Arc<AtomicBool>,
}

/// Receiving half — held by the flush thread.
pub struct FlushReceiver {
    /// Receiving half of the channel.
    rx: Receiver<FlushSignal>,
    /// Shared stop flag.
    stop: Arc<AtomicBool>,
}

/// Signals that can be sent to the flush thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlushSignal {
    /// Run a flush cycle immediately (e.g. dirty watermark exceeded).
    Wake,
    /// Perform a final flush and then terminate the thread.
    Shutdown,
}

/// Create a linked `(FlushNotifier, FlushReceiver)` pair.
pub fn flush_channel() -> (FlushNotifier, FlushReceiver) {
    // Bounded(1) so that a second Wake doesn't block — it just means
    // "flush soon" which one pending wake already covers.
    let (tx, rx) = channel::bounded(1);
    let stop = Arc::new(AtomicBool::new(false));

    let notifier = FlushNotifier {
        tx,
        stop: Arc::clone(&stop),
    };
    let receiver = FlushReceiver { rx, stop };

    (notifier, receiver)
}

impl FlushNotifier {
    /// Signal the flush thread to run one cycle immediately.
    ///
    /// Non-blocking: if a wake is already pending, this is a no-op.
    pub fn wake(&self) {
        let _ = self.tx.try_send(FlushSignal::Wake);
    }

    /// Signal the flush thread to perform a final flush and exit.
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.tx.try_send(FlushSignal::Shutdown);
    }

    /// Check whether shutdown has been requested.
    pub fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
}

impl FlushReceiver {
    /// Block until a signal arrives or `timeout` elapses.
    ///
    /// Returns `Some(signal)` if one was received, or `None` on timeout.
    pub fn wait(&self, timeout: std::time::Duration) -> Option<FlushSignal> {
        match self.rx.recv_timeout(timeout) {
            Ok(signal) => Some(signal),
            Err(crossbeam::channel::RecvTimeoutError::Timeout) => None,
            Err(crossbeam::channel::RecvTimeoutError::Disconnected) => {
                // Sender dropped — treat as shutdown.
                Some(FlushSignal::Shutdown)
            }
        }
    }

    /// Check whether shutdown has been requested (non-blocking).
    pub fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
}
