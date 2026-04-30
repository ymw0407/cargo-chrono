//! System resource monitoring for the TUI dashboard.
//!
//! Periodically samples CPU usage and memory consumption via `sysinfo`
//! and forwards snapshots to the TUI through a channel.
//!
//! All sampling runs inside a dedicated async task so the rendering loop
//! is never blocked by OS calls.

use std::time::Duration;

use sysinfo::System;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// A point-in-time snapshot of system resource usage.
#[derive(Debug, Clone)]
pub struct SystemSnapshot {
    /// Overall CPU usage across all logical cores, in percent (0.0–100.0).
    pub cpu_usage_percent: f32,
    /// Physical memory currently in use, in bytes.
    pub mem_used_bytes: u64,
    /// Total physical memory available, in bytes.
    pub mem_total_bytes: u64,
}

/// Wraps `sysinfo::System` and exposes a single `sample` method.
///
/// The internal `System` must be refreshed twice with a brief pause between
/// calls for `cpu_usage_percent` to reflect real activity; the first call
/// establishes the baseline.  Callers driving a periodic loop (e.g.
/// `run_system_monitor`) naturally satisfy this requirement.
pub struct SystemMonitor {
    sys: System,
}

impl SystemMonitor {
    /// Create a new monitor.
    ///
    /// Performs an initial system refresh so that the first call to
    /// [`sample`](Self::sample) produces a non-zero memory reading.
    /// CPU usage on the first sample may read 0% because there is no
    /// prior baseline; subsequent calls will be accurate.
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self { sys }
    }

    /// Refresh system data and return a new snapshot.
    ///
    /// # Returns
    ///
    /// A [`SystemSnapshot`] with the current CPU and memory readings.
    pub fn sample(&mut self) -> SystemSnapshot {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        SystemSnapshot {
            cpu_usage_percent: self.sys.global_cpu_usage(),
            mem_used_bytes: self.sys.used_memory(),
            mem_total_bytes: self.sys.total_memory(),
        }
    }
}

impl Default for SystemMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Run a periodic system-monitoring task.
///
/// Samples system resources at `interval` and sends each [`SystemSnapshot`]
/// to `tx`.  Exits cleanly when:
/// - `cancel` is triggered, or
/// - `tx` is closed (the receiver was dropped).
///
/// # Arguments
///
/// * `tx`       — Channel to forward snapshots to the TUI state.
/// * `interval` — How often to sample.  Typical value: `Duration::from_secs(1)`.
/// * `cancel`   — Cancellation token shared with the rest of the application.
///
/// # Errors
///
/// Returns `Ok(())` on all clean-shutdown paths.  No I/O errors are
/// expected from `sysinfo` itself; if they occur they are silently ignored.
pub async fn run_system_monitor(
    tx: mpsc::Sender<SystemSnapshot>,
    interval: Duration,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let mut monitor = SystemMonitor::new();

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Ok(()),
            _ = tokio::time::sleep(interval) => {
                let snapshot = monitor.sample();
                // If the receiver is gone the TUI has already shut down.
                if tx.send(snapshot).await.is_err() {
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn new_does_not_panic() {
        let _monitor = SystemMonitor::new();
    }

    #[test]
    fn sample_returns_valid_ranges() {
        let mut monitor = SystemMonitor::new();
        let snap = monitor.sample();

        assert!(
            snap.cpu_usage_percent >= 0.0 && snap.cpu_usage_percent <= 100.0,
            "cpu_usage_percent out of range: {}",
            snap.cpu_usage_percent
        );
        assert!(
            snap.mem_total_bytes > 0,
            "mem_total_bytes should be non-zero on any real machine"
        );
        assert!(
            snap.mem_used_bytes <= snap.mem_total_bytes,
            "used ({}) > total ({})",
            snap.mem_used_bytes,
            snap.mem_total_bytes
        );
    }

    #[test]
    fn consecutive_samples_do_not_panic() {
        let mut monitor = SystemMonitor::new();
        for _ in 0..3 {
            let _ = monitor.sample();
        }
    }

    #[tokio::test]
    async fn run_exits_on_cancel() {
        let (tx, _rx) = mpsc::channel(4);
        let cancel = CancellationToken::new();

        let handle = tokio::spawn(run_system_monitor(
            tx,
            Duration::from_secs(60), // long interval — won't fire
            cancel.clone(),
        ));

        cancel.cancel();

        let result = timeout(Duration::from_secs(1), handle)
            .await
            .expect("run_system_monitor did not exit after cancel");
        result.unwrap().unwrap();
    }

    #[tokio::test]
    async fn run_exits_when_receiver_dropped() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx); // close the receiver immediately
        let cancel = CancellationToken::new();

        let handle = tokio::spawn(run_system_monitor(
            tx,
            Duration::from_millis(10), // short so the send fires quickly
            cancel,
        ));

        let result = timeout(Duration::from_secs(2), handle)
            .await
            .expect("run_system_monitor did not exit after receiver drop");
        result.unwrap().unwrap();
    }

    #[tokio::test]
    async fn run_sends_snapshots_to_channel() {
        let (tx, mut rx) = mpsc::channel(4);
        let cancel = CancellationToken::new();

        tokio::spawn(run_system_monitor(
            tx,
            Duration::from_millis(50),
            cancel.clone(),
        ));

        let snap = timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for first snapshot")
            .expect("channel closed before first snapshot");

        assert!(snap.mem_total_bytes > 0);
        cancel.cancel();
    }
}
