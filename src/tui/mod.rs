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

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::anomaly;
use crate::model::{Baseline, BuildEvent};
use crate::persist::BuildRepository;
use crate::tui::state::TuiState;

/// Block in the TUI's keyboard loop until the user presses any key, or
/// `cancel` is fired. Used after the build finishes so the final dashboard
/// stays on screen instead of vanishing the moment the alt screen is left.
///
/// **Important:** this function must NOT cancel the shared `CancellationToken`
/// on key press. The build is already finished by the time we wait here, and
/// firing the token would route the run through `finalize_or_discard`'s
/// "interrupted" branch, deleting the just-recorded build. The outer TUI loop
/// already has an explicit `break` after this call, so no signal is needed.
fn wait_for_exit_key(cancel: &CancellationToken) -> anyhow::Result<()> {
    loop {
        if cancel.is_cancelled() {
            return Ok(());
        }
        if crossterm::event::poll(Duration::from_millis(100))? {
            if let Event::Key(_) = crossterm::event::read()? {
                return Ok(());
            }
        }
    }
}

/// Look up a crate's baseline, caching the result for the lifetime of the run.
///
/// Baselines are immutable for the duration of a build, so the per-tick
/// in-progress classifier and the per-crate finished classifier can share
/// one lookup. Without this cache, the ~60 fps tick loop would issue an
/// async DB query for every active crate on every frame.
async fn cached_baseline<'a>(
    cache: &'a mut HashMap<String, Option<Baseline>>,
    repo: &dyn BuildRepository,
    name: &str,
) -> Option<&'a Baseline> {
    if !cache.contains_key(name) {
        let fetched = repo.fetch_baseline(name).await.ok().flatten();
        cache.insert(name.to_string(), fetched);
    }
    cache.get(name).and_then(|b| b.as_ref())
}

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
        let _ = crossterm::execute!(std::io::stderr(), crossterm::terminal::LeaveAlternateScreen);
        original_hook(info);
    }));

    // R11: RAII guard as a second safety net for normal exit paths.
    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let _ = crossterm::terminal::disable_raw_mode();
            let _ =
                crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
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
    let mut baseline_cache: HashMap<String, Option<Baseline>> = HashMap::new();

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
                            let duration = state
                                .recent
                                .iter()
                                .rev()
                                .find(|c| c.crate_id == crate_id)
                                .map(|c| c.duration);
                            if let Some(duration) = duration {
                                let baseline =
                                    cached_baseline(&mut baseline_cache, &*repo, &crate_id.name)
                                        .await;
                                let verdict = anomaly::classify(duration, baseline, 2.0);
                                state.set_verdict(&crate_id, verdict);
                            }
                        }

                        // Final render when the build is complete (R12: render
                        // before breaking so the user sees the finished state).
                        // Wait for a keypress so the result stays on screen
                        // even when the build was a fast cache hit.
                        if state.is_finished() {
                            terminal.draw(|f| render::render_dashboard(f, &state))?;
                            wait_for_exit_key(&cancel)?;
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
                // In raw mode Ctrl-C arrives as a key event, not SIGINT, so it
                // must be matched explicitly.
                if crossterm::event::poll(Duration::ZERO)? {
                    if let Event::Key(key) = crossterm::event::read()? {
                        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Char('Q') => {
                                cancel.cancel();
                                break;
                            }
                            KeyCode::Char('c') | KeyCode::Char('C') if ctrl => {
                                cancel.cancel();
                                break;
                            }
                            _ => {}
                        }
                    }
                }

                // Refresh in-progress anomaly verdicts for active crates.
                // Baselines come from the cache, so this loop performs no DB
                // I/O after each crate's first lookup — keeping the 60 fps
                // tick from being blocked by repo awaits.
                let active_ids: Vec<_> = state.active.keys().cloned().collect();
                for crate_id in active_ids {
                    let elapsed = match state.active.get(&crate_id) {
                        Some(a) => a.elapsed(),
                        None => continue,
                    };
                    let baseline =
                        cached_baseline(&mut baseline_cache, &*repo, &crate_id.name).await;
                    let verdict = anomaly::classify_in_progress(elapsed, baseline, 2.0);
                    state.set_in_progress_verdict(&crate_id, verdict);
                }

                terminal.draw(|f| render::render_dashboard(f, &state))?;
            },
        }
    }

    Ok(())
    // _guard drops here → disable_raw_mode + LeaveAlternateScreen (R11, R12)
}
