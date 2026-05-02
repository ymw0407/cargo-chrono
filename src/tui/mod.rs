//! Real-time TUI dashboard for monitoring builds.
//!
//! Renders a terminal UI showing:
//! - Currently compiling crates with elapsed time
//! - Anomaly indicators (slow/fast/normal) per crate
//! - Overall build progress and crate count
//! - CPU and memory usage
//!
//! # Contract
//!
//! - Targets ~60 fps rendering via a `tokio::time::interval` tick.
//! - Restores the terminal to normal mode on `Drop` (raw mode cleanup).
//! - Exits on `q`, `Ctrl-C`, or when the `CancellationToken` is triggered.
//! - Consumes `BuildEvent`s from a channel and uses `BuildRepository` for
//!   baseline lookups.

pub mod render;
pub mod state;
pub mod system_monitor;

use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, KeyCode};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::anomaly;
use crate::model::BuildEvent;
use crate::persist::BuildRepository;
use crate::tui::state::TuiState;

/// Run the TUI dashboard until the build finishes, the user quits, or
/// `cancel` is triggered.
///
/// # Terminal handling
///
/// Enables raw mode and enters the alternate screen on start.  A RAII
/// [`TerminalGuard`] and a custom panic hook together guarantee that the
/// terminal is restored even if a panic occurs (R11 from CONCURRENCY.md).
///
/// # Arguments
///
/// * `events` — Channel of build events from the broker.
/// * `repo`   — Repository for fetching crate baselines (read-only).
/// * `cancel` — Shared cancellation token for graceful shutdown.
///
/// # Errors
///
/// Returns an error if terminal initialisation fails or if an unrecoverable
/// rendering error occurs.  Terminal cleanup still runs via the RAII guard
/// before the error propagates.
pub async fn run_tui(
    mut events: mpsc::Receiver<BuildEvent>,
    repo: Arc<dyn BuildRepository>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    // R11: register panic hook so raw mode is restored even on panic.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stderr(),
            crossterm::terminal::LeaveAlternateScreen
        );
        original_hook(info);
    }));

    // R11: RAII guard as a second safety net for normal exit paths.
    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let _ = crossterm::terminal::disable_raw_mode();
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::terminal::LeaveAlternateScreen
            );
        }
    }
    let _guard = TerminalGuard;

    // Initialise terminal.
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = TuiState::new();

    // Launch system monitor as a background task.
    let (sys_tx, mut sys_rx) = mpsc::channel(4);
    tokio::spawn(system_monitor::run_system_monitor(
        sys_tx,
        Duration::from_secs(1),
        cancel.clone(),
    ));

    // ~60 fps render tick (16 ms).
    let mut tick = tokio::time::interval(Duration::from_millis(16));

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,

            maybe_event = events.recv() => {
                match maybe_event {
                    Some(event) => {
                        let finished_ids = state.apply_event(&event);

                        // For each newly finished crate, look up its baseline
                        // and classify the duration.
                        for crate_id in finished_ids {
                            if let Some(comp) = state.recent.iter().rev().find(|c| c.crate_id == crate_id) {
                                let duration = comp.duration;
                                if let Ok(Some(baseline)) = repo.fetch_baseline(&crate_id.name).await {
                                    let verdict = anomaly::classify(duration, Some(&baseline), 2.0);
                                    state.set_verdict(&crate_id, verdict);
                                }
                            }
                        }

                        // Final render when the build is complete (R12: render
                        // before breaking so the user sees the finished state).
                        if state.is_finished() {
                            terminal.draw(|f| render::render_dashboard(f, &state))?;
                            break;
                        }
                    }
                    // Broker closed its channel — build stream ended.
                    None => break,
                }
            },

            Some(snap) = sys_rx.recv() => {
                state.update_system(snap);
            },

            _ = tick.tick() => {
                // Non-blocking keyboard check (crossterm synchronous poll).
                if crossterm::event::poll(Duration::ZERO)? {
                    if let Event::Key(key) = crossterm::event::read()? {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Char('Q') => {
                                cancel.cancel();
                                break;
                            }
                            _ => {}
                        }
                    }
                }

                // Refresh in-progress anomaly verdicts for active crates.
                let active_ids: Vec<_> = state.active.keys().cloned().collect();
                for crate_id in active_ids {
                    if let Some(active) = state.active.get(&crate_id) {
                        let elapsed = active.elapsed();
                        if let Ok(Some(baseline)) = repo.fetch_baseline(&crate_id.name).await {
                            let verdict =
                                anomaly::classify_in_progress(elapsed, Some(&baseline), 2.0);
                            state.set_in_progress_verdict(&crate_id, verdict);
                        }
                    }
                }

                terminal.draw(|f| render::render_dashboard(f, &state))?;
            },
        }
    }

    Ok(())
    // _guard drops here → disable_raw_mode + LeaveAlternateScreen (R11, R12)
}
