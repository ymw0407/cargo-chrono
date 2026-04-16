//! Types representing the result of comparing two builds.

use std::time::Duration;

use crate::model::ids::{BuildId, CrateId};

/// The complete result of diffing two builds.
#[derive(Debug, Clone)]
pub struct BuildDiff {
    /// The earlier build being compared.
    pub before: BuildId,
    /// The later build being compared.
    pub after: BuildId,
    /// Change in total build duration.
    pub total_change: DurationChange,
    /// Per-crate changes, sorted by absolute delta descending.
    pub crate_changes: Vec<CrateChange>,
    /// Crate names on the critical path of the "before" build.
    pub critical_path_before: Vec<String>,
    /// Crate names on the critical path of the "after" build.
    pub critical_path_after: Vec<String>,
}

/// Represents a change in duration between two builds.
#[derive(Debug, Clone)]
pub struct DurationChange {
    pub before: Duration,
    pub after: Duration,
    /// Signed delta in milliseconds (positive = slower).
    pub abs_delta_ms: i64,
    /// Percentage change (positive = slower). e.g. 17.6 means 17.6% slower.
    pub pct_delta: f64,
}

/// How a single crate changed between two builds.
#[derive(Debug, Clone)]
pub enum CrateChange {
    /// Crate was not present in the "before" build.
    Added {
        crate_id: CrateId,
        duration: Duration,
    },
    /// Crate was present in the "before" build but not in "after".
    Removed {
        crate_id: CrateId,
        duration: Duration,
    },
    /// Crate was present in both builds with a measurable difference.
    Changed {
        crate_id: CrateId,
        change: DurationChange,
    },
    /// Crate was present in both builds with negligible difference.
    Unchanged {
        crate_id: CrateId,
        duration: Duration,
    },
}
