//! Real-time TUI dashboard for monitoring builds.
//!
//! Renders a terminal UI showing:
//! - Currently compiling crates with elapsed time
//! - Anomaly indicators (slow/fast/normal) per crate
//! - Overall build progress and ETA
//! - CPU and memory usage
//!
//! # Contract
//!
//! - Targets ~60fps rendering via ratatui's event loop.
//! - Restores the terminal to normal mode on `Drop` (raw mode cleanup).
//! - Exits on `q`, `Ctrl-C`, or when the `CancellationToken` is triggered.
//! - Consumes `BuildEvent`s from a channel and uses `BuildRepository` for baseline lookups.

pub mod render;
pub mod state;
pub mod system_monitor;

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::model::BuildEvent;
use crate::persist::BuildRepository;

/// Run the TUI dashboard.
///
/// # Arguments
///
/// * `events` — Channel of build events from the broker.
/// * `repo` — Repository for fetching crate baselines (read-only).
/// * `cancel` — Cancellation token for graceful shutdown.
///
/// # Terminal handling
///
/// Enters raw mode and alternate screen on start. On exit (normal or panic),
/// restores the terminal to its original state.
///
/// # Errors
///
/// Returns an error if terminal initialization fails or if an unrecoverable
/// rendering error occurs.
pub async fn run_tui(
    _events: mpsc::Receiver<BuildEvent>,
    _repo: Arc<dyn BuildRepository>,
    _cancel: CancellationToken,
) -> anyhow::Result<()> {
    todo!("Initialize terminal, enter event loop, render frames at ~60fps")
}
