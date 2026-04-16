//! Build diff computation.
//!
//! Compares two recorded builds to identify which crates got slower/faster,
//! which were added/removed, and how the critical path changed.

pub mod critical_path;

use crate::model::{BuildDiff, BuildId};
use crate::persist::BuildRepository;

/// Compare two builds and produce a detailed diff.
///
/// # Arguments
///
/// * `repo` — Repository to fetch build details from.
/// * `before` — The earlier build ID.
/// * `after` — The later build ID.
///
/// # Returns
///
/// A `BuildDiff` containing:
/// - Total duration change
/// - Per-crate changes (added, removed, changed, unchanged)
/// - Critical path for each build
///
/// # Errors
///
/// Returns an error if either build ID does not exist in the repository
/// or if a database query fails.
pub async fn compute_diff(
    _repo: &dyn BuildRepository,
    _before: BuildId,
    _after: BuildId,
) -> anyhow::Result<BuildDiff> {
    todo!("Fetch both builds, compare crate-by-crate, compute critical paths")
}
