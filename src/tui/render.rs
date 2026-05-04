//! TUI rendering logic.
//!
//! Builds ratatui widgets from [`TuiState`] and draws them to the terminal
//! frame.  The main entry point is [`render_dashboard`], called by the event
//! loop in [`super::run_tui`] on every render tick (~60 fps).
//!
//! Layout (top to bottom):
//! ```text
//! ┌─ cargo-chronoscope ────────────────────────────────────────────┐
//! │ Build #N (release)  •  commit abc1234  •  elapsed 0:28   │
//! │ 142 crates compiled                                        │
//! ├─ Active compilations ─────────────────────────────────────┤
//! │  ▶ serde_derive        12.4s   ⚠ slower                  │
//! ├─ Recently finished (last 5) ──────────────────────────────┤
//! │  ✓ syn                  5.8s   · normal                   │
//! ├─ System   [q] quit  [Ctrl-C] interrupt ───────────────────┤
//! │  CPU:  75.5%   Memory: 4.0 GiB / 16.0 GiB               │
//! └───────────────────────────────────────────────────────────┘
//! ```

use std::time::Duration;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::anomaly::AnomalyVerdict;
use crate::tui::state::TuiState;

/// Render the full dashboard to `frame` from the current `state`.
///
/// # Arguments
///
/// * `frame` — The ratatui frame to draw into.
/// * `state` — Current TUI application state snapshot.
pub fn render_dashboard(frame: &mut Frame, state: &TuiState) {
    let area = frame.area();

    let chunks = Layout::new(
        Direction::Vertical,
        [
            Constraint::Length(4), // header: build info + crate count
            Constraint::Min(3),    // active compilations (expands as needed)
            Constraint::Length(7), // recently finished (5 rows + borders)
            Constraint::Length(3), // system info + footer hint
        ],
    )
    .split(area);

    render_header(frame, chunks[0], state);
    render_active(frame, chunks[1], state);
    render_recent(frame, chunks[2], state);
    render_system(frame, chunks[3], state);
}

fn render_header(frame: &mut Frame, area: Rect, state: &TuiState) {
    let id_str = state
        .build_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "?".to_string());
    let profile_str = state
        .profile
        .as_ref()
        .map(|p| p.to_string())
        .unwrap_or_else(|| "dev".to_string());
    let commit_str = state
        .commit_hash
        .as_deref()
        .map(|h| format!("commit {}", &h[..h.len().min(7)]))
        .unwrap_or_else(|| "no commit".to_string());
    let elapsed_str = state
        .elapsed()
        .map(format_duration)
        .unwrap_or_else(|| "0:00".to_string());

    let title_line = format!(
        "Build {} ({})  •  {}  •  elapsed {}",
        id_str, profile_str, commit_str, elapsed_str
    );

    let status_line = if state.is_finished() {
        let symbol = if state.build_result == Some(true) {
            "✓ Build succeeded"
        } else {
            "✗ Build failed"
        };
        let total = state
            .total_duration
            .map(format_duration)
            .unwrap_or_default();
        format!(
            "{}  in {}  ({} crates)",
            symbol, total, state.finished_count
        )
    } else {
        format!("{} crates compiled", state.finished_count)
    };

    let text = vec![
        Line::from(Span::styled(
            title_line,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(status_line),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" cargo-chronoscope ");

    frame.render_widget(Paragraph::new(text).block(block), area);
}

fn render_active(frame: &mut Frame, area: Rect, state: &TuiState) {
    let mut lines: Vec<Line> = Vec::new();

    if state.active.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no active compilations)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        // Collect and sort by elapsed descending so the slowest sits at top.
        let mut entries: Vec<_> = state.active.values().map(|c| (c, c.elapsed())).collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.1));

        for (comp, elapsed) in entries {
            let verdict = verdict_label(comp.verdict);
            let color = verdict_color(comp.verdict);
            lines.push(Line::from(Span::styled(
                format!(
                    "  ▶ {:<32} {:>6}   {}",
                    comp.crate_id.to_string(),
                    format_duration(elapsed),
                    verdict
                ),
                Style::default().fg(color),
            )));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Active compilations ");

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_recent(frame: &mut Frame, area: Rect, state: &TuiState) {
    let mut lines: Vec<Line> = Vec::new();

    if state.recent.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none yet)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        // Newest first.
        for comp in state.recent.iter().rev() {
            let verdict = verdict_label(comp.verdict);
            let color = verdict_color(comp.verdict);
            lines.push(Line::from(Span::styled(
                format!(
                    "  ✓ {:<32} {:>6}   {}",
                    comp.crate_id.to_string(),
                    format_duration(comp.duration),
                    verdict
                ),
                Style::default().fg(color),
            )));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Recently finished (last 5) ");

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_system(frame: &mut Frame, area: Rect, state: &TuiState) {
    let line = if let Some(snap) = &state.system {
        format!(
            "  CPU: {:>5.1}%   Memory: {} / {}",
            snap.cpu_usage_percent,
            format_bytes(snap.mem_used_bytes),
            format_bytes(snap.mem_total_bytes),
        )
    } else {
        "  CPU: --   Memory: --".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" System   [q] quit  [Ctrl-C] interrupt ");

    frame.render_widget(Paragraph::new(vec![Line::from(line)]).block(block), area);
}

// ── Public helpers ────────────────────────────────────────────────────────────

/// Format a [`Duration`] as `M:SS` or `H:MM:SS`.
///
/// # Returns
///
/// `"0:42"` for 42 s, `"1:30"` for 90 s, `"1:01:01"` for 3661 s.
pub fn format_duration(d: Duration) -> String {
    let total = d.as_secs();
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{}:{:02}", m, s)
    }
}

/// Format a byte count as a human-readable string.
///
/// # Returns
///
/// The value in GiB / MiB / KiB / B, rounded to one decimal place.
pub fn format_bytes(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    const KIB: u64 = 1024;

    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Return a short display label for an [`AnomalyVerdict`].
///
/// # Returns
///
/// `"⚠ slower"`, `"↓ faster"`, `"· normal"`, or `"? unknown"`.
pub fn verdict_label(verdict: AnomalyVerdict) -> &'static str {
    match verdict {
        AnomalyVerdict::Slower => "⚠ slower",
        AnomalyVerdict::Faster => "↓ faster",
        AnomalyVerdict::Normal => "· normal",
        AnomalyVerdict::Unknown => "? unknown",
    }
}

fn verdict_color(verdict: AnomalyVerdict) -> Color {
    match verdict {
        AnomalyVerdict::Slower => Color::Red,
        AnomalyVerdict::Faster => Color::Green,
        AnomalyVerdict::Normal => Color::Reset,
        AnomalyVerdict::Unknown => Color::DarkGray,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BuildEvent, BuildId, BuildProfile, CrateId, CrateKind};
    use crate::tui::system_monitor::SystemSnapshot;
    use ratatui::{backend::TestBackend, Terminal};

    fn make_terminal() -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(80, 30)).unwrap()
    }

    fn buf(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    fn crate_id(name: &str) -> CrateId {
        CrateId {
            name: name.to_string(),
            version: None,
        }
    }

    fn started(name: &str) -> BuildEvent {
        BuildEvent::CompilationStarted {
            crate_id: crate_id(name),
        }
    }

    fn finished(name: &str, ms: u64) -> BuildEvent {
        BuildEvent::CompilationFinished {
            crate_id: crate_id(name),
            kind: CrateKind::Lib,
            started_at: "2025-01-01T00:00:00Z".into(),
            finished_at: "2025-01-01T00:00:01Z".into(),
            duration: Duration::from_millis(ms),
        }
    }

    // ── render_dashboard ─────────────────────────────────────────────────────

    #[test]
    fn empty_state_does_not_panic() {
        let mut t = make_terminal();
        let state = TuiState::new();
        t.draw(|f| render_dashboard(f, &state)).unwrap();
    }

    #[test]
    fn header_shows_build_id_when_set() {
        let mut t = make_terminal();
        let mut state = TuiState::new();
        state.build_id = Some(BuildId(42));
        t.draw(|f| render_dashboard(f, &state)).unwrap();
        assert!(buf(&t).contains("#42"), "expected '#42' in buffer");
    }

    #[test]
    fn header_shows_profile_from_build_started() {
        let mut t = make_terminal();
        let mut state = TuiState::new();
        state.apply_event(&BuildEvent::BuildStarted {
            at: "2025-01-01T00:00:00Z".into(),
            commit_hash: Some("deadbeef".into()),
            cargo_args: vec![],
            profile: BuildProfile::Release,
        });
        t.draw(|f| render_dashboard(f, &state)).unwrap();
        assert!(buf(&t).contains("release"));
    }

    #[test]
    fn header_shows_commit_prefix() {
        let mut t = make_terminal();
        let mut state = TuiState::new();
        state.apply_event(&BuildEvent::BuildStarted {
            at: "2025-01-01T00:00:00Z".into(),
            commit_hash: Some("abc1234def".into()),
            cargo_args: vec![],
            profile: BuildProfile::Dev,
        });
        t.draw(|f| render_dashboard(f, &state)).unwrap();
        // Only the first 7 chars are shown.
        assert!(buf(&t).contains("abc1234"));
    }

    #[test]
    fn active_section_shows_crate_name() {
        let mut t = make_terminal();
        let mut state = TuiState::new();
        state.apply_event(&started("serde_derive"));
        t.draw(|f| render_dashboard(f, &state)).unwrap();
        assert!(buf(&t).contains("serde_derive"));
    }

    #[test]
    fn active_section_shows_slower_verdict() {
        let mut t = make_terminal();
        let mut state = TuiState::new();
        state.apply_event(&started("heavy_crate"));
        state.set_in_progress_verdict(&crate_id("heavy_crate"), AnomalyVerdict::Slower);
        t.draw(|f| render_dashboard(f, &state)).unwrap();
        assert!(buf(&t).contains("slower"));
    }

    #[test]
    fn recent_section_shows_finished_crate_name() {
        let mut t = make_terminal();
        let mut state = TuiState::new();
        state.apply_event(&started("syn"));
        state.apply_event(&finished("syn", 1500));
        t.draw(|f| render_dashboard(f, &state)).unwrap();
        assert!(buf(&t).contains("syn"));
    }

    #[test]
    fn recent_section_shows_faster_verdict() {
        let mut t = make_terminal();
        let mut state = TuiState::new();
        state.apply_event(&started("quote"));
        state.apply_event(&finished("quote", 200));
        state.set_verdict(&crate_id("quote"), AnomalyVerdict::Faster);
        t.draw(|f| render_dashboard(f, &state)).unwrap();
        assert!(buf(&t).contains("faster"));
    }

    #[test]
    fn system_section_shows_cpu_when_snapshot_present() {
        let mut t = make_terminal();
        let mut state = TuiState::new();
        state.update_system(SystemSnapshot {
            cpu_usage_percent: 75.5,
            mem_used_bytes: 4 * 1024 * 1024 * 1024,
            mem_total_bytes: 16 * 1024 * 1024 * 1024,
        });
        t.draw(|f| render_dashboard(f, &state)).unwrap();
        let text = buf(&t);
        assert!(text.contains("75.5"), "expected CPU% in buffer");
        assert!(text.contains("GiB"), "expected GiB in buffer");
    }

    #[test]
    fn system_section_shows_placeholder_without_snapshot() {
        let mut t = make_terminal();
        let state = TuiState::new();
        t.draw(|f| render_dashboard(f, &state)).unwrap();
        assert!(buf(&t).contains("--"));
    }

    #[test]
    fn finished_build_shows_success_status() {
        let mut t = make_terminal();
        let mut state = TuiState::new();
        state.apply_event(&BuildEvent::BuildFinished {
            success: true,
            total_duration: Duration::from_secs(30),
            at: "2025-01-01T00:00:30Z".into(),
        });
        t.draw(|f| render_dashboard(f, &state)).unwrap();
        assert!(buf(&t).contains("succeeded"));
    }

    #[test]
    fn finished_build_shows_failure_status() {
        let mut t = make_terminal();
        let mut state = TuiState::new();
        state.apply_event(&BuildEvent::BuildFinished {
            success: false,
            total_duration: Duration::from_secs(5),
            at: "2025-01-01T00:00:05Z".into(),
        });
        t.draw(|f| render_dashboard(f, &state)).unwrap();
        assert!(buf(&t).contains("failed"));
    }

    // ── format_duration ───────────────────────────────────────────────────────

    #[test]
    fn format_duration_zero() {
        assert_eq!(format_duration(Duration::ZERO), "0:00");
    }

    #[test]
    fn format_duration_seconds_only() {
        assert_eq!(format_duration(Duration::from_secs(42)), "0:42");
    }

    #[test]
    fn format_duration_over_one_minute() {
        assert_eq!(format_duration(Duration::from_secs(90)), "1:30");
    }

    #[test]
    fn format_duration_over_one_hour() {
        assert_eq!(format_duration(Duration::from_secs(3661)), "1:01:01");
    }

    #[test]
    fn format_duration_exactly_one_minute() {
        assert_eq!(format_duration(Duration::from_secs(60)), "1:00");
    }

    // ── format_bytes ──────────────────────────────────────────────────────────

    #[test]
    fn format_bytes_below_kib() {
        assert_eq!(format_bytes(500), "500 B");
    }

    #[test]
    fn format_bytes_kib_boundary() {
        assert_eq!(format_bytes(2048), "2.0 KiB");
    }

    #[test]
    fn format_bytes_mib() {
        assert_eq!(format_bytes(512 * 1024 * 1024), "512.0 MiB");
    }

    #[test]
    fn format_bytes_gib() {
        assert_eq!(format_bytes(4 * 1024 * 1024 * 1024), "4.0 GiB");
    }

    // ── verdict_label ─────────────────────────────────────────────────────────

    #[test]
    fn verdict_labels_all_variants() {
        assert_eq!(verdict_label(AnomalyVerdict::Slower), "⚠ slower");
        assert_eq!(verdict_label(AnomalyVerdict::Faster), "↓ faster");
        assert_eq!(verdict_label(AnomalyVerdict::Normal), "· normal");
        assert_eq!(verdict_label(AnomalyVerdict::Unknown), "? unknown");
    }
}
