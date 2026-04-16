//! Event broker for fan-out distribution of `BuildEvent`s.
//!
//! In `watch` mode, the broker sits between the Parser and multiple consumers
//! (Persister and TUI), cloning each event to all subscribers.
//!
//! # Usage
//!
//! ```ignore
//! let mut broker = EventBroker::new();
//! let persister_rx = broker.subscribe(1024);
//! let tui_rx = broker.subscribe(1024);
//! broker.publish_loop(parser_rx, cancel_token).await?;
//! ```

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::model::BuildEvent;

/// Distributes `BuildEvent`s to multiple subscribers.
///
/// Each subscriber receives a clone of every event. If a subscriber's
/// channel is full or closed, it is automatically removed.
pub struct EventBroker {
    subscribers: Vec<mpsc::Sender<BuildEvent>>,
}

impl EventBroker {
    /// Create a new broker with no subscribers.
    pub fn new() -> Self {
        Self {
            subscribers: Vec::new(),
        }
    }

    /// Register a new subscriber and return its event receiver.
    ///
    /// # Arguments
    ///
    /// * `buffer` — Bounded channel capacity for this subscriber.
    pub fn subscribe(&mut self, buffer: usize) -> mpsc::Receiver<BuildEvent> {
        let (tx, rx) = mpsc::channel(buffer);
        self.subscribers.push(tx);
        rx
    }

    /// Consume events from `rx` and broadcast to all subscribers.
    ///
    /// Runs until either:
    /// - The input channel `rx` is closed (all events consumed).
    /// - The `cancel` token is triggered (Ctrl-C or user quit).
    ///
    /// Subscribers whose channels are full or closed are silently removed.
    ///
    /// # Errors
    ///
    /// Returns `Ok(())` on clean shutdown. Returns an error only if
    /// an unexpected internal failure occurs.
    pub async fn publish_loop(
        self,
        _rx: mpsc::Receiver<BuildEvent>,
        _cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        todo!("Read from rx, clone event to each subscriber, remove dead subscribers")
    }
}

impl Default for EventBroker {
    fn default() -> Self {
        Self::new()
    }
}
