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
        mut self,
        mut rx: mpsc::Receiver<BuildEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Ok(()),
                maybe_event = rx.recv() => {
                    let event = match maybe_event {
                        Some(e) => e,
                        None => return Ok(()),
                    };
                    // Fan-out: try_send avoids blocking on slow/full subscribers (R4).
                    // Full  → drop this event for that subscriber only.
                    // Closed → permanently remove the subscriber.
                    self.subscribers.retain(|tx| {
                        match tx.try_send(event.clone()) {
                            Ok(()) => true,
                            Err(mpsc::error::TrySendError::Full(_)) => true,
                            Err(mpsc::error::TrySendError::Closed(_)) => false,
                        }
                    });
                }
            }
        }
    }
}

impl Default for EventBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    //! Contract tests for `EventBroker::publish_loop`.
    //!
    //! These tests will panic at `todo!()` until `publish_loop` is implemented.

    use super::*;
    use crate::model::BuildEvent;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use tokio::time::timeout;
    use tokio_util::sync::CancellationToken;

    fn sample_build_finished() -> BuildEvent {
        BuildEvent::BuildFinished {
            success: true,
            total_duration: Duration::from_secs(1),
            at: "2025-04-19T12:00:00Z".to_string(),
        }
    }

    #[tokio::test]
    async fn every_subscriber_receives_every_event() {
        let mut broker = EventBroker::new();
        let mut rx1 = broker.subscribe(16);
        let mut rx2 = broker.subscribe(16);

        let (tx, rx) = mpsc::channel::<BuildEvent>(16);
        let cancel = CancellationToken::new();
        let handle = tokio::spawn(broker.publish_loop(rx, cancel.clone()));

        tx.send(sample_build_finished()).await.unwrap();
        drop(tx); // closes input → publish_loop should drain and exit

        let ev1 = timeout(Duration::from_secs(1), rx1.recv())
            .await
            .expect("rx1 receive timed out");
        let ev2 = timeout(Duration::from_secs(1), rx2.recv())
            .await
            .expect("rx2 receive timed out");
        assert!(
            matches!(ev1, Some(BuildEvent::BuildFinished { .. })),
            "rx1 got {:?}",
            ev1
        );
        assert!(
            matches!(ev2, Some(BuildEvent::BuildFinished { .. })),
            "rx2 got {:?}",
            ev2
        );

        // publish_loop should return Ok(()) on clean shutdown.
        let result = timeout(Duration::from_secs(1), handle)
            .await
            .expect("publish_loop did not exit");
        result.unwrap().unwrap();
    }

    #[tokio::test]
    async fn cancel_token_stops_the_loop() {
        let broker = EventBroker::new();
        let (_tx, rx) = mpsc::channel::<BuildEvent>(16);
        let cancel = CancellationToken::new();

        let handle = tokio::spawn(broker.publish_loop(rx, cancel.clone()));
        cancel.cancel();

        let result = timeout(Duration::from_secs(1), handle)
            .await
            .expect("publish_loop did not exit after cancel");
        result.unwrap().unwrap();
    }

    #[tokio::test]
    async fn closed_input_channel_exits_cleanly() {
        let broker = EventBroker::new();
        let (tx, rx) = mpsc::channel::<BuildEvent>(16);
        let cancel = CancellationToken::new();
        let handle = tokio::spawn(broker.publish_loop(rx, cancel));
        drop(tx);
        let result = timeout(Duration::from_secs(1), handle)
            .await
            .expect("publish_loop did not exit after input closed");
        result.unwrap().unwrap();
    }

    #[tokio::test]
    async fn dead_subscriber_does_not_block_others() {
        // If one subscriber drops its receiver, the other should still receive events.
        let mut broker = EventBroker::new();
        let dead_rx = broker.subscribe(16);
        let mut live_rx = broker.subscribe(16);
        drop(dead_rx); // subscriber 1 is dead before any publishing happens

        let (tx, rx) = mpsc::channel::<BuildEvent>(16);
        let cancel = CancellationToken::new();
        let handle = tokio::spawn(broker.publish_loop(rx, cancel));

        tx.send(sample_build_finished()).await.unwrap();
        drop(tx);

        let received = timeout(Duration::from_secs(1), live_rx.recv())
            .await
            .expect("live subscriber timed out");
        assert!(matches!(received, Some(BuildEvent::BuildFinished { .. })));

        let _ = timeout(Duration::from_secs(1), handle).await;
    }

    #[tokio::test]
    async fn subscribe_without_publish_does_not_panic() {
        // Smoke test: creating subscribers before `publish_loop` runs must not panic.
        let mut broker = EventBroker::new();
        let _rx1 = broker.subscribe(16);
        let _rx2 = broker.subscribe(16);
    }
}
