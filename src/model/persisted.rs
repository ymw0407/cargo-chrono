//! Types representing data stored in and retrieved from the database.

use std::time::Duration;

use crate::model::ids::{BuildId, CrateId};

/// A recorded build summary as stored in the `builds` table.
#[derive(Debug, Clone)]
pub struct Build {
    pub id: BuildId,
    /// ISO 8601 timestamp.
    pub started_at: String,
    /// ISO 8601 timestamp. `None` if the build was interrupted.
    pub finished_at: Option<String>,
    /// Git commit hash at the time of the build.
    pub commit_hash: Option<String>,
    /// The cargo arguments used for this build (serialized as JSON).
    pub cargo_args: String,
    /// Build profile name ("dev", "release", "custom").
    pub profile: String,
    /// Whether the build succeeded. `None` if not yet finished.
    pub success: Option<bool>,
    /// Total wall-clock duration. `None` if not yet finished.
    pub total_duration: Option<Duration>,
}

/// A single crate compilation record as stored in the `crate_compilations` table.
#[derive(Debug, Clone)]
pub struct CrateCompilation {
    pub build_id: BuildId,
    pub crate_id: CrateId,
    /// Target kind (e.g. "lib", "bin", "build-script").
    pub kind: String,
    /// ISO 8601 timestamp.
    pub started_at: String,
    /// ISO 8601 timestamp.
    pub finished_at: String,
    /// Wall-clock compilation duration.
    pub duration: Duration,
}

/// A build with all its crate compilation details.
#[derive(Debug, Clone)]
pub struct BuildDetails {
    pub build: Build,
    pub compilations: Vec<CrateCompilation>,
}

/// Statistical baseline for a crate's compilation time, computed from historical data.
///
/// Used by the anomaly detector to classify current compilations as normal/slow/fast.
#[derive(Debug, Clone)]
pub struct Baseline {
    pub crate_id: CrateId,
    /// Number of historical samples used to compute this baseline.
    pub sample_count: u32,
    /// Mean compilation duration.
    pub mean: Duration,
    /// Standard deviation of compilation duration.
    pub std_dev: Duration,
    /// Minimum observed compilation duration.
    pub min: Duration,
    /// Maximum observed compilation duration.
    pub max: Duration,
}
