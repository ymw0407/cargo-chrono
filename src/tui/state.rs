//! TUI application state.
//!
//! Holds all data the rendering pass needs to draw the dashboard: active
//! compilations, recently finished compilations, anomaly verdicts, build
//! metadata, and system metrics.
//!
//! State is mutated from two independent sources:
//! 1. [`TuiState::apply_event`] — called for each incoming [`BuildEvent`].
//! 2. [`TuiState::set_verdict`] / [`TuiState::set_in_progress_verdict`] /
//!    [`TuiState::update_system`] — called when async work (baseline lookups,
//!    system sampling) completes.
//!
//! All mutation is synchronous.  The async event loop in `tui/mod.rs` is
//! responsible for driving the async side.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crate::anomaly::AnomalyVerdict;
use crate::model::{BuildEvent, BuildId, BuildProfile, CrateId, CrateKind};
use crate::tui::system_monitor::SystemSnapshot;

/// Maximum number of recently-finished compilations kept in the dashboard.
const MAX_RECENT: usize = 5;

/// A crate that is currently being compiled.
#[derive(Debug, Clone)]
pub struct ActiveCompilation {
    /// The crate being compiled.
    pub crate_id: CrateId,
    /// The compilation target kind.
    pub kind: CrateKind,
    /// Monotonic clock instant when the compilation started.
    ///
    /// Used to compute [`elapsed`](Self::elapsed) for the live timer column.
    pub started_at: Instant,
    /// In-progress anomaly classification.
    ///
    /// Starts as [`AnomalyVerdict::Unknown`].  Updated periodically by the
    /// event loop via [`TuiState::set_in_progress_verdict`] using
    /// [`anomaly::classify_in_progress`](crate::anomaly::classify_in_progress).
    pub verdict: AnomalyVerdict,
}

impl ActiveCompilation {
    /// Returns wall-clock time elapsed since this compilation started.
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }
}

/// A crate compilation that has just finished.
#[derive(Debug, Clone)]
pub struct FinishedCompilation {
    /// The crate that finished compiling.
    pub crate_id: CrateId,
    /// The compilation target kind.
    pub kind: CrateKind,
    /// Total wall-clock compilation duration (from the Parser).
    pub duration: Duration,
    /// Anomaly classification against historical baseline.
    ///
    /// Starts as [`AnomalyVerdict::Unknown`] and is updated by the event loop
    /// via [`TuiState::set_verdict`] once the async baseline lookup completes.
    pub verdict: AnomalyVerdict,
}

/// All mutable state the TUI rendering pass reads from.
#[derive(Debug)]
pub struct TuiState {
    /// Database-assigned build ID, set once available via [`set_build_id`](Self::set_build_id).
    pub build_id: Option<BuildId>,
    /// Build profile (dev / release / custom), set on [`BuildEvent::BuildStarted`].
    pub profile: Option<BuildProfile>,
    /// Git commit hash at build time, if the build was inside a git repo.
    pub commit_hash: Option<String>,
    /// Monotonic clock instant when the build started.
    pub started_at: Option<Instant>,

    /// Crates currently being compiled, keyed by [`CrateId`].
    pub active: HashMap<CrateId, ActiveCompilation>,

    /// Up to [`MAX_RECENT`] most recently finished compilations (oldest first).
    pub recent: VecDeque<FinishedCompilation>,

    /// Running count of compilations that have finished so far.
    pub finished_count: usize,

    /// Latest system resource reading, or `None` before the first sample.
    pub system: Option<SystemSnapshot>,

    /// `None` while the build is running; `Some(success)` after it finishes.
    pub build_result: Option<bool>,
    /// Total build duration, populated on [`BuildEvent::BuildFinished`].
    pub total_duration: Option<Duration>,
}

impl TuiState {
    /// Create a fresh, empty state.
    pub fn new() -> Self {
        Self {
            build_id: None,
            profile: None,
            commit_hash: None,
            started_at: None,
            active: HashMap::new(),
            recent: VecDeque::new(),
            finished_count: 0,
            system: None,
            build_result: None,
            total_duration: None,
        }
    }

    /// Apply a [`BuildEvent`] to the state, mutating it in place.
    ///
    /// # Returns
    ///
    /// A list of [`CrateId`]s whose compilations just finished.  The caller
    /// should use these to schedule async baseline lookups and then call
    /// [`set_verdict`](Self::set_verdict) with the result.
    ///
    /// Returns an empty `Vec` for events that don't finish any compilations.
    pub fn apply_event(&mut self, event: &BuildEvent) -> Vec<CrateId> {
        match event {
            BuildEvent::BuildStarted {
                commit_hash,
                profile,
                ..
            } => {
                self.profile = Some(profile.clone());
                self.commit_hash = commit_hash.clone();
                self.started_at = Some(Instant::now());
                vec![]
            }

            BuildEvent::CompilationStarted { crate_id, kind, .. } => {
                self.active.insert(
                    crate_id.clone(),
                    ActiveCompilation {
                        crate_id: crate_id.clone(),
                        kind: kind.clone(),
                        started_at: Instant::now(),
                        verdict: AnomalyVerdict::Unknown,
                    },
                );
                vec![]
            }

            BuildEvent::CompilationFinished {
                crate_id,
                kind,
                duration,
                ..
            } => {
                self.active.remove(crate_id);
                self.finished_count += 1;

                if self.recent.len() >= MAX_RECENT {
                    self.recent.pop_front();
                }
                self.recent.push_back(FinishedCompilation {
                    crate_id: crate_id.clone(),
                    kind: kind.clone(),
                    duration: *duration,
                    verdict: AnomalyVerdict::Unknown,
                });

                vec![crate_id.clone()]
            }

            BuildEvent::BuildFinished {
                success,
                total_duration,
                ..
            } => {
                self.build_result = Some(*success);
                self.total_duration = Some(*total_duration);
                self.active.clear();
                vec![]
            }

            BuildEvent::CompilerMessage { .. } => vec![],
        }
    }

    /// Update the anomaly verdict for a finished compilation.
    ///
    /// Matches the most recently pushed entry in `recent` for `crate_id`,
    /// which is correct because each crate typically compiles once per build.
    ///
    /// # Arguments
    ///
    /// * `crate_id` — Crate returned earlier by [`apply_event`](Self::apply_event).
    /// * `verdict`  — Result of [`anomaly::classify`](crate::anomaly::classify).
    pub fn set_verdict(&mut self, crate_id: &CrateId, verdict: AnomalyVerdict) {
        if let Some(entry) = self.recent.iter_mut().rev().find(|c| &c.crate_id == crate_id) {
            entry.verdict = verdict;
        }
    }

    /// Update the in-progress anomaly verdict for an active compilation.
    ///
    /// Called periodically by the event loop so the dashboard can show
    /// `⚠ slower` for crates that are already running long.
    ///
    /// # Arguments
    ///
    /// * `crate_id` — An active crate (still in [`active`](Self::active)).
    /// * `verdict`  — Result of [`anomaly::classify_in_progress`](crate::anomaly::classify_in_progress).
    pub fn set_in_progress_verdict(&mut self, crate_id: &CrateId, verdict: AnomalyVerdict) {
        if let Some(entry) = self.active.get_mut(crate_id) {
            entry.verdict = verdict;
        }
    }

    /// Replace the current system resource snapshot.
    ///
    /// # Arguments
    ///
    /// * `snapshot` — A fresh reading from [`SystemMonitor::sample`](super::system_monitor::SystemMonitor::sample).
    pub fn update_system(&mut self, snapshot: SystemSnapshot) {
        self.system = Some(snapshot);
    }

    /// Store the database-assigned [`BuildId`].
    ///
    /// The ID is issued by the Persister on `BuildStarted` and is not part of
    /// the event stream visible to the TUI.  The event loop must pass it in
    /// via a side channel if real-time display is desired.
    ///
    /// # Arguments
    ///
    /// * `id` — The build ID assigned by [`BuildRepository::begin_build`](crate::persist::BuildRepository::begin_build).
    pub fn set_build_id(&mut self, id: BuildId) {
        self.build_id = Some(id);
    }

    /// Returns the elapsed time since the build started, or `None` if not started.
    pub fn elapsed(&self) -> Option<Duration> {
        self.started_at.map(|t| t.elapsed())
    }

    /// Returns `true` if the build has finished (success or failure).
    pub fn is_finished(&self) -> bool {
        self.build_result.is_some()
    }
}

impl Default for TuiState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BuildProfile, CrateKind};
    use std::time::Duration;

    fn crate_id(name: &str) -> CrateId {
        CrateId {
            name: name.to_string(),
            version: None,
        }
    }

    fn build_started() -> BuildEvent {
        BuildEvent::BuildStarted {
            at: "2025-01-01T00:00:00Z".into(),
            commit_hash: Some("abc123".into()),
            cargo_args: vec!["build".into()],
            profile: BuildProfile::Release,
        }
    }

    fn compilation_started(name: &str) -> BuildEvent {
        BuildEvent::CompilationStarted {
            crate_id: crate_id(name),
            kind: CrateKind::Lib,
            at: "2025-01-01T00:00:00Z".into(),
        }
    }

    fn compilation_finished(name: &str, ms: u64) -> BuildEvent {
        BuildEvent::CompilationFinished {
            crate_id: crate_id(name),
            kind: CrateKind::Lib,
            started_at: "2025-01-01T00:00:00Z".into(),
            finished_at: "2025-01-01T00:00:01Z".into(),
            duration: Duration::from_millis(ms),
        }
    }

    fn build_finished(success: bool) -> BuildEvent {
        BuildEvent::BuildFinished {
            success,
            total_duration: Duration::from_secs(5),
            at: "2025-01-01T00:00:05Z".into(),
        }
    }

    #[test]
    fn new_state_is_empty() {
        let s = TuiState::new();
        assert!(s.build_id.is_none());
        assert!(s.profile.is_none());
        assert!(s.active.is_empty());
        assert!(s.recent.is_empty());
        assert_eq!(s.finished_count, 0);
        assert!(s.system.is_none());
        assert!(!s.is_finished());
        assert!(s.elapsed().is_none());
    }

    #[test]
    fn build_started_sets_metadata() {
        let mut s = TuiState::new();
        s.apply_event(&build_started());

        assert_eq!(s.profile, Some(BuildProfile::Release));
        assert_eq!(s.commit_hash.as_deref(), Some("abc123"));
        assert!(s.started_at.is_some());
        assert!(s.elapsed().is_some());
    }

    #[test]
    fn compilation_started_adds_to_active() {
        let mut s = TuiState::new();
        let finished = s.apply_event(&compilation_started("serde"));

        assert!(finished.is_empty());
        assert!(s.active.contains_key(&crate_id("serde")));
        assert_eq!(s.active.len(), 1);
    }

    #[test]
    fn compilation_finished_moves_to_recent_and_returns_crate_id() {
        let mut s = TuiState::new();
        s.apply_event(&compilation_started("serde"));
        let finished = s.apply_event(&compilation_finished("serde", 1000));

        assert_eq!(finished, vec![crate_id("serde")]);
        assert!(s.active.is_empty());
        assert_eq!(s.recent.len(), 1);
        assert_eq!(s.recent[0].crate_id, crate_id("serde"));
        assert_eq!(s.recent[0].duration, Duration::from_millis(1000));
        assert_eq!(s.finished_count, 1);
    }

    #[test]
    fn build_finished_clears_active_and_sets_result() {
        let mut s = TuiState::new();
        s.apply_event(&compilation_started("foo"));
        s.apply_event(&build_finished(true));

        assert!(s.active.is_empty());
        assert_eq!(s.build_result, Some(true));
        assert!(s.is_finished());
        assert_eq!(s.total_duration, Some(Duration::from_secs(5)));
    }

    #[test]
    fn compiler_message_returns_empty_vec_and_does_not_mutate() {
        let mut s = TuiState::new();
        let before_count = s.finished_count;
        let finished = s.apply_event(&BuildEvent::CompilerMessage {
            crate_id: crate_id("foo"),
            level: crate::model::MessageLevel::Warning,
            text: "unused var".into(),
        });

        assert!(finished.is_empty());
        assert_eq!(s.finished_count, before_count);
    }

    #[test]
    fn recent_is_capped_at_max() {
        let mut s = TuiState::new();
        for i in 0..=MAX_RECENT {
            let name = format!("crate{}", i);
            s.apply_event(&compilation_started(&name));
            s.apply_event(&compilation_finished(&name, 100));
        }

        assert_eq!(s.recent.len(), MAX_RECENT);
        // Oldest entry (crate0) should have been evicted.
        assert!(!s.recent.iter().any(|c| c.crate_id.name == "crate0"));
    }

    #[test]
    fn set_verdict_updates_most_recent_match() {
        let mut s = TuiState::new();
        s.apply_event(&compilation_started("serde"));
        s.apply_event(&compilation_finished("serde", 1000));

        s.set_verdict(&crate_id("serde"), AnomalyVerdict::Slower);

        let entry = s.recent.iter().find(|c| c.crate_id.name == "serde").unwrap();
        assert_eq!(entry.verdict, AnomalyVerdict::Slower);
    }

    #[test]
    fn set_verdict_on_unknown_crate_does_not_panic() {
        let mut s = TuiState::new();
        s.set_verdict(&crate_id("nonexistent"), AnomalyVerdict::Slower);
    }

    #[test]
    fn set_in_progress_verdict_updates_active_entry() {
        let mut s = TuiState::new();
        s.apply_event(&compilation_started("tokio"));

        s.set_in_progress_verdict(&crate_id("tokio"), AnomalyVerdict::Slower);

        assert_eq!(s.active[&crate_id("tokio")].verdict, AnomalyVerdict::Slower);
    }

    #[test]
    fn set_in_progress_verdict_on_unknown_crate_does_not_panic() {
        let mut s = TuiState::new();
        s.set_in_progress_verdict(&crate_id("ghost"), AnomalyVerdict::Normal);
    }

    #[test]
    fn update_system_stores_snapshot() {
        let mut s = TuiState::new();
        assert!(s.system.is_none());

        let snap = SystemSnapshot {
            cpu_usage_percent: 42.0,
            mem_used_bytes: 1024,
            mem_total_bytes: 8192,
        };
        s.update_system(snap);

        let stored = s.system.as_ref().unwrap();
        assert!((stored.cpu_usage_percent - 42.0).abs() < f32::EPSILON);
    }

    #[test]
    fn set_build_id_is_stored() {
        let mut s = TuiState::new();
        s.set_build_id(BuildId(7));
        assert_eq!(s.build_id, Some(BuildId(7)));
    }

    #[test]
    fn active_compilation_elapsed_is_non_negative() {
        let mut s = TuiState::new();
        s.apply_event(&compilation_started("syn"));
        let elapsed = s.active[&crate_id("syn")].elapsed();
        assert!(elapsed >= Duration::ZERO);
    }

    #[test]
    fn full_build_sequence_state_transitions() {
        let mut s = TuiState::new();

        s.apply_event(&build_started());
        assert!(s.started_at.is_some());
        assert!(!s.is_finished());

        s.apply_event(&compilation_started("proc-macro2"));
        s.apply_event(&compilation_started("syn"));
        assert_eq!(s.active.len(), 2);

        s.apply_event(&compilation_finished("proc-macro2", 800));
        assert_eq!(s.active.len(), 1);
        assert_eq!(s.finished_count, 1);

        s.apply_event(&compilation_finished("syn", 2000));
        assert_eq!(s.active.len(), 0);
        assert_eq!(s.finished_count, 2);

        s.apply_event(&build_finished(true));
        assert!(s.is_finished());
        assert_eq!(s.build_result, Some(true));
    }
}
