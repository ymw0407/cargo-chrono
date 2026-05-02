//! Build diff computation.
//!
//! Compares two recorded builds to identify which crates got slower/faster,
//! which were added/removed, and how the critical path changed.

pub mod critical_path;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::model::{
    BuildDetails, BuildDiff, BuildId, CrateChange, CrateCompilation, CrateId, DurationChange,
};
use crate::persist::BuildRepository;

/// Two crate durations within this many milliseconds of each other are treated
/// as equivalent and reported as `CrateChange::Unchanged`.
///
/// Compilation timing has measurement noise (system load, I/O variance), so
/// we don't surface tiny deltas as "changes". Tune as needed.
const UNCHANGED_THRESHOLD_MS: i64 = 5;

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
    repo: &dyn BuildRepository,
    before: BuildId,
    after: BuildId,
) -> anyhow::Result<BuildDiff> {
    // 1) Pull both builds. A missing ID is an error — the caller asked us to
    //    diff something that doesn't exist.
    let before_details = repo
        .fetch_build(before)
        .await?
        .ok_or_else(|| anyhow::anyhow!("build {} not found", before))?;
    let after_details = repo
        .fetch_build(after)
        .await?
        .ok_or_else(|| anyhow::anyhow!("build {} not found", after))?;

    // 2) Crate-by-crate comparison. We key on `CrateId` (name + version) so that
    //    different versions of the same crate count as distinct entries.
    let crate_changes = build_crate_changes(&before_details, &after_details);

    // 3) Total duration change. If a build was interrupted (None), fall back to
    //    zero so we still produce a consistent DurationChange struct.
    let before_total = before_details.build.total_duration.unwrap_or_default();
    let after_total = after_details.build.total_duration.unwrap_or_default();
    let total_change = duration_change(before_total, after_total);

    // 4) Critical paths.
    let critical_path_before = critical_path::compute_critical_path(&before_details.compilations);
    let critical_path_after = critical_path::compute_critical_path(&after_details.compilations);

    Ok(BuildDiff {
        before,
        after,
        total_change,
        crate_changes,
        critical_path_before,
        critical_path_after,
    })
}

/// Build the per-crate change list, sorted by absolute delta (largest first).
///
/// Walks the union of crates in both builds and classifies each into one of
/// the four `CrateChange` variants.
fn build_crate_changes(before: &BuildDetails, after: &BuildDetails) -> Vec<CrateChange> {
    // CrateId is Hash + Eq, so HashMap is the natural choice. Final ordering
    // is decided by the sort step below, not iteration order here.
    let before_map: HashMap<CrateId, &CrateCompilation> = before
        .compilations
        .iter()
        .map(|c| (c.crate_id.clone(), c))
        .collect();
    let after_map: HashMap<CrateId, &CrateCompilation> = after
        .compilations
        .iter()
        .map(|c| (c.crate_id.clone(), c))
        .collect();

    // Union of keys from both sides.
    let mut all_keys: HashSet<CrateId> = HashSet::new();
    all_keys.extend(before_map.keys().cloned());
    all_keys.extend(after_map.keys().cloned());

    let mut changes: Vec<CrateChange> = all_keys
        .into_iter()
        .map(|crate_id| {
            match (before_map.get(&crate_id), after_map.get(&crate_id)) {
                // Present in both → Changed if delta exceeds the threshold,
                // otherwise Unchanged.
                (Some(b), Some(a)) => {
                    let change = duration_change(b.duration, a.duration);
                    if change.abs_delta_ms.abs() > UNCHANGED_THRESHOLD_MS {
                        CrateChange::Changed { crate_id, change }
                    } else {
                        CrateChange::Unchanged {
                            crate_id,
                            duration: a.duration,
                        }
                    }
                }
                // Only in `before` → removed.
                (Some(b), None) => CrateChange::Removed {
                    crate_id,
                    duration: b.duration,
                },
                // Only in `after` → added.
                (None, Some(a)) => CrateChange::Added {
                    crate_id,
                    duration: a.duration,
                },
                // Should be unreachable — the key came from one of the maps.
                (None, None) => unreachable!("crate id absent from both build maps"),
            }
        })
        .collect();

    // Sort by impact — biggest change first. Added/Removed are scored by their
    // own duration, Changed by its absolute delta, Unchanged by zero so they
    // sink to the bottom.
    changes.sort_by_key(|c| std::cmp::Reverse(change_magnitude(c)));
    changes
}

/// Score used to order `CrateChange` entries by impact (descending).
fn change_magnitude(change: &CrateChange) -> i64 {
    match change {
        CrateChange::Changed { change, .. } => change.abs_delta_ms.abs(),
        CrateChange::Added { duration, .. } | CrateChange::Removed { duration, .. } => {
            duration.as_millis() as i64
        }
        CrateChange::Unchanged { .. } => 0,
    }
}

/// Compute a `DurationChange` between two durations.
///
/// `abs_delta_ms` is signed (positive = `after` is slower).
/// `pct_delta` is `0.0` when `before` is zero, to avoid division by zero.
fn duration_change(before: Duration, after: Duration) -> DurationChange {
    let before_ms = before.as_millis() as i64;
    let after_ms = after.as_millis() as i64;
    let abs_delta_ms = after_ms - before_ms;

    let pct_delta = if before_ms == 0 {
        0.0
    } else {
        (abs_delta_ms as f64 / before_ms as f64) * 100.0
    };

    DurationChange {
        before,
        after,
        abs_delta_ms,
        pct_delta,
    }
}

#[cfg(test)]
mod tests {
    //! Contract tests for `compute_diff`.
    //!
    //! Uses a real `SqliteRepository` (with a tempdir) as the fixture, so these
    //! tests additionally depend on `SqliteRepository` CRUD being implemented.

    use super::*;
    use crate::model::{BuildProfile, CrateChange, CrateId};
    use crate::persist::{BuildRepository, SqliteRepository};
    use std::time::Duration;
    use tempfile::TempDir;

    struct Fixture {
        _dir: TempDir,
        repo: SqliteRepository,
    }

    async fn setup() -> Fixture {
        let dir = TempDir::new().unwrap();
        let repo = SqliteRepository::open(&dir.path().join("test.db"))
            .await
            .unwrap();
        Fixture { _dir: dir, repo }
    }

    /// Records a build with the given (crate_name, duration_ms) entries.
    async fn record_build(
        repo: &SqliteRepository,
        started_at: &str,
        crates: &[(&str, u64)],
    ) -> BuildId {
        let id = repo
            .begin_build(started_at, None, "[]", &BuildProfile::Dev)
            .await
            .unwrap();
        for (name, ms) in crates {
            let crate_id = CrateId {
                name: (*name).to_string(),
                version: None,
            };
            repo.record_compilation(
                id,
                &crate_id,
                "lib",
                started_at,
                started_at,
                Duration::from_millis(*ms),
            )
            .await
            .unwrap();
        }
        let total: u64 = crates.iter().map(|(_, ms)| ms).sum();
        repo.finalize_build(id, started_at, true, Duration::from_millis(total))
            .await
            .unwrap();
        id
    }

    #[tokio::test]
    async fn identical_builds_produce_only_unchanged_entries() {
        let fx = setup().await;
        let a = record_build(
            &fx.repo,
            "2025-01-01T00:00:00Z",
            &[("foo", 100), ("bar", 200)],
        )
        .await;
        let b = record_build(
            &fx.repo,
            "2025-01-02T00:00:00Z",
            &[("foo", 100), ("bar", 200)],
        )
        .await;

        let diff = compute_diff(&fx.repo, a, b).await.unwrap();
        assert_eq!(diff.before, a);
        assert_eq!(diff.after, b);
        for change in &diff.crate_changes {
            assert!(
                matches!(change, CrateChange::Unchanged { .. }),
                "expected Unchanged, got {:?}",
                change
            );
        }
    }

    #[tokio::test]
    async fn crate_only_in_after_is_added() {
        let fx = setup().await;
        let a = record_build(&fx.repo, "2025-01-01T00:00:00Z", &[("foo", 100)]).await;
        let b = record_build(
            &fx.repo,
            "2025-01-02T00:00:00Z",
            &[("foo", 100), ("new", 50)],
        )
        .await;

        let diff = compute_diff(&fx.repo, a, b).await.unwrap();
        let has_added = diff
            .crate_changes
            .iter()
            .any(|c| matches!(c, CrateChange::Added { crate_id, .. } if crate_id.name == "new"));
        assert!(
            has_added,
            "expected 'new' to be Added; got {:?}",
            diff.crate_changes
        );
    }

    #[tokio::test]
    async fn crate_only_in_before_is_removed() {
        let fx = setup().await;
        let a = record_build(
            &fx.repo,
            "2025-01-01T00:00:00Z",
            &[("foo", 100), ("old", 50)],
        )
        .await;
        let b = record_build(&fx.repo, "2025-01-02T00:00:00Z", &[("foo", 100)]).await;

        let diff = compute_diff(&fx.repo, a, b).await.unwrap();
        let has_removed = diff
            .crate_changes
            .iter()
            .any(|c| matches!(c, CrateChange::Removed { crate_id, .. } if crate_id.name == "old"));
        assert!(
            has_removed,
            "expected 'old' to be Removed; got {:?}",
            diff.crate_changes
        );
    }

    #[tokio::test]
    async fn slower_crate_reports_positive_delta_and_pct() {
        let fx = setup().await;
        let a = record_build(&fx.repo, "2025-01-01T00:00:00Z", &[("foo", 100)]).await;
        let b = record_build(&fx.repo, "2025-01-02T00:00:00Z", &[("foo", 200)]).await;

        let diff = compute_diff(&fx.repo, a, b).await.unwrap();
        let change = diff.crate_changes.iter().find_map(|c| match c {
            CrateChange::Changed { crate_id, change } if crate_id.name == "foo" => Some(change),
            _ => None,
        });
        let ch = change.expect("expected 'foo' to be Changed");
        assert_eq!(ch.abs_delta_ms, 100);
        assert!(
            ch.pct_delta > 0.0,
            "pct_delta should be positive, got {}",
            ch.pct_delta
        );
    }

    #[tokio::test]
    async fn faster_crate_reports_negative_delta() {
        let fx = setup().await;
        let a = record_build(&fx.repo, "2025-01-01T00:00:00Z", &[("foo", 300)]).await;
        let b = record_build(&fx.repo, "2025-01-02T00:00:00Z", &[("foo", 100)]).await;

        let diff = compute_diff(&fx.repo, a, b).await.unwrap();
        let change = diff.crate_changes.iter().find_map(|c| match c {
            CrateChange::Changed { crate_id, change } if crate_id.name == "foo" => Some(change),
            _ => None,
        });
        let ch = change.expect("expected 'foo' to be Changed");
        assert_eq!(ch.abs_delta_ms, -200);
        assert!(
            ch.pct_delta < 0.0,
            "pct_delta should be negative, got {}",
            ch.pct_delta
        );
    }

    #[tokio::test]
    async fn total_change_reflects_total_duration_delta() {
        let fx = setup().await;
        let a = record_build(&fx.repo, "2025-01-01T00:00:00Z", &[("foo", 1000)]).await;
        let b = record_build(&fx.repo, "2025-01-02T00:00:00Z", &[("foo", 1500)]).await;

        let diff = compute_diff(&fx.repo, a, b).await.unwrap();
        assert_eq!(diff.total_change.abs_delta_ms, 500);
        assert!(diff.total_change.pct_delta > 0.0);
    }

    #[tokio::test]
    async fn missing_build_returns_error() {
        let fx = setup().await;
        let a = record_build(&fx.repo, "2025-01-01T00:00:00Z", &[("foo", 100)]).await;
        let result = compute_diff(&fx.repo, a, BuildId(9999)).await;
        assert!(result.is_err(), "expected error for missing build");
    }
}
