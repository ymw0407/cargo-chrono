//! SQLite-backed implementation of `BuildRepository`.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use rusqlite::Connection;
use tokio::sync::Mutex;

use crate::model::{Baseline, Build, BuildDetails, BuildId, BuildProfile, CrateId};
use crate::persist::BuildRepository;

use super::migrations;

/// SQLite-backed build repository.
///
/// Stores all build data in a single SQLite file with WAL mode enabled
/// for concurrent read access. The `Connection` is wrapped in a `Mutex`
/// because `rusqlite::Connection` is not `Sync`.
pub struct SqliteRepository {
    conn: Mutex<Connection>,
}

impl SqliteRepository {
    /// Open (or create) a SQLite database at the given path.
    ///
    /// Enables WAL journal mode for better concurrent read performance
    /// and runs any pending schema migrations.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or if migrations fail.
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;

        // Enable WAL mode for better concurrent read performance.
        conn.pragma_update(None, "journal_mode", "wal")?;

        // Run schema migrations.
        migrations::run_migrations(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

#[async_trait]
impl BuildRepository for SqliteRepository {
    async fn begin_build(
        &self,
        _started_at: &str,
        _commit_hash: Option<&str>,
        _cargo_args: &str,
        _profile: &BuildProfile,
    ) -> anyhow::Result<BuildId> {
        todo!("INSERT into builds and return BuildId")
    }

    async fn record_compilation(
        &self,
        _build_id: BuildId,
        _crate_id: &CrateId,
        _kind: &str,
        _started_at: &str,
        _finished_at: &str,
        _duration: Duration,
    ) -> anyhow::Result<()> {
        todo!("INSERT into crate_compilations")
    }

    async fn finalize_build(
        &self,
        _build_id: BuildId,
        _finished_at: &str,
        _success: bool,
        _total_duration: Duration,
    ) -> anyhow::Result<()> {
        todo!("UPDATE builds SET finished_at, success, total_duration_ms")
    }

    async fn list_builds(&self, _limit: usize) -> anyhow::Result<Vec<Build>> {
        todo!("SELECT from builds ORDER BY started_at DESC LIMIT ?")
    }

    async fn fetch_build(&self, _id: BuildId) -> anyhow::Result<Option<BuildDetails>> {
        todo!("SELECT build + JOIN crate_compilations")
    }

    async fn fetch_baseline(&self, _crate_name: &str) -> anyhow::Result<Option<Baseline>> {
        todo!("Compute mean, std_dev, min, max from historical crate_compilations")
    }
}

#[cfg(test)]
mod tests {
    //! Contract tests for `SqliteRepository`.
    //!
    //! `test_open_creates_tables` passes as-is (the real impl is done).
    //! The rest will panic at `todo!()` until the CRUD methods are implemented —
    //! they serve as a spec for the Data team.

    use super::*;
    use tempfile::TempDir;

    async fn fresh_repo() -> (TempDir, SqliteRepository) {
        let dir = TempDir::new().unwrap();
        let repo = SqliteRepository::open(&dir.path().join("test.db"))
            .await
            .unwrap();
        (dir, repo)
    }

    fn lib_crate(name: &str) -> CrateId {
        CrateId {
            name: name.to_string(),
            version: None,
        }
    }

    #[tokio::test]
    async fn test_open_creates_tables() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let repo = SqliteRepository::open(&db_path).await.unwrap();

        let conn = repo.conn.lock().await;

        // Verify that the builds table exists.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM builds", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);

        // Verify that the crate_compilations table exists.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM crate_compilations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn begin_build_returns_strictly_increasing_ids() {
        let (_d, repo) = fresh_repo().await;
        let id1 = repo
            .begin_build("2025-01-01T00:00:00Z", None, "[]", &BuildProfile::Dev)
            .await
            .unwrap();
        let id2 = repo
            .begin_build("2025-01-01T00:01:00Z", None, "[]", &BuildProfile::Dev)
            .await
            .unwrap();
        assert!(
            id2.0 > id1.0,
            "expected monotonic IDs, got {:?} then {:?}",
            id1,
            id2
        );
    }

    #[tokio::test]
    async fn record_and_finalize_roundtrip_via_fetch_build() {
        let (_d, repo) = fresh_repo().await;
        let id = repo
            .begin_build(
                "2025-01-01T00:00:00Z",
                Some("deadbeef"),
                r#"["build"]"#,
                &BuildProfile::Dev,
            )
            .await
            .unwrap();
        let crate_id = CrateId {
            name: "demo".into(),
            version: Some("0.1.0".into()),
        };
        repo.record_compilation(
            id,
            &crate_id,
            "lib",
            "2025-01-01T00:00:00Z",
            "2025-01-01T00:00:05Z",
            Duration::from_millis(5000),
        )
        .await
        .unwrap();
        repo.finalize_build(
            id,
            "2025-01-01T00:00:05Z",
            true,
            Duration::from_millis(5000),
        )
        .await
        .unwrap();

        let details = repo.fetch_build(id).await.unwrap().expect("build exists");
        assert_eq!(details.build.id, id);
        assert_eq!(details.build.commit_hash.as_deref(), Some("deadbeef"));
        assert_eq!(details.build.success, Some(true));
        assert_eq!(
            details.build.total_duration,
            Some(Duration::from_millis(5000))
        );
        assert_eq!(details.compilations.len(), 1);
        assert_eq!(details.compilations[0].crate_id.name, "demo");
        assert_eq!(details.compilations[0].kind, "lib");
        assert_eq!(
            details.compilations[0].duration,
            Duration::from_millis(5000)
        );
    }

    #[tokio::test]
    async fn fetch_build_missing_returns_none() {
        let (_d, repo) = fresh_repo().await;
        let result = repo.fetch_build(BuildId(9999)).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn list_builds_returns_most_recent_first_respects_limit() {
        let (_d, repo) = fresh_repo().await;
        let timestamps = [
            "2025-01-01T00:00:00Z",
            "2025-01-02T00:00:00Z",
            "2025-01-03T00:00:00Z",
        ];
        for ts in timestamps.iter() {
            repo.begin_build(ts, None, "[]", &BuildProfile::Dev)
                .await
                .unwrap();
        }

        let builds = repo.list_builds(2).await.unwrap();
        assert_eq!(builds.len(), 2);
        assert_eq!(builds[0].started_at, "2025-01-03T00:00:00Z");
        assert_eq!(builds[1].started_at, "2025-01-02T00:00:00Z");
    }

    #[tokio::test]
    async fn list_builds_returns_empty_when_no_rows() {
        let (_d, repo) = fresh_repo().await;
        let builds = repo.list_builds(10).await.unwrap();
        assert!(builds.is_empty());
    }

    #[tokio::test]
    async fn fetch_baseline_none_when_no_data() {
        let (_d, repo) = fresh_repo().await;
        let result = repo.fetch_baseline("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn fetch_baseline_computes_mean_min_max() {
        let (_d, repo) = fresh_repo().await;
        let id = repo
            .begin_build("2025-01-01T00:00:00Z", None, "[]", &BuildProfile::Dev)
            .await
            .unwrap();
        let crate_id = lib_crate("foo");

        for ms in [100u64, 200, 300, 400, 500] {
            repo.record_compilation(
                id,
                &crate_id,
                "lib",
                "2025-01-01T00:00:00Z",
                "2025-01-01T00:00:01Z",
                Duration::from_millis(ms),
            )
            .await
            .unwrap();
        }

        let baseline = repo
            .fetch_baseline("foo")
            .await
            .unwrap()
            .expect("baseline should exist with 5 samples");

        assert_eq!(baseline.sample_count, 5);
        assert_eq!(baseline.mean, Duration::from_millis(300));
        assert_eq!(baseline.min, Duration::from_millis(100));
        assert_eq!(baseline.max, Duration::from_millis(500));

        // std_dev for {100,200,300,400,500} is ~141ms (population) or ~158ms (sample).
        // Accept either convention — just ensure it's in a sensible range.
        let std_ms = baseline.std_dev.as_millis() as f64;
        assert!(
            (100.0..=200.0).contains(&std_ms),
            "std_dev out of expected range: {}ms",
            std_ms
        );
    }
}
