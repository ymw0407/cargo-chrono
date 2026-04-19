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
