//! SQLite-backed implementation of `BuildRepository`.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use rusqlite::Connection;
use tokio::sync::Mutex;

use crate::model::{
    Baseline, Build, BuildDetails, BuildId, BuildProfile, CrateCompilation, CrateId,
};
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
    /// - Sets a 5s `busy_timeout` so writer-lock contention from another
    ///   `cargo-chronoscope` process retries automatically instead of failing
    ///   with `SQLITE_BUSY`.
    /// - Enables WAL journal mode for better concurrent read performance.
    /// - Runs any pending schema migrations atomically (see
    ///   [`migrations::run_migrations`]).
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or if migrations fail.
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        let mut conn = Connection::open(path)?;

        // Block (rather than instantly failing) on writer-lock contention.
        conn.busy_timeout(Duration::from_secs(5))?;

        // Enable WAL mode for better concurrent read performance.
        conn.pragma_update(None, "journal_mode", "wal")?;

        // Run schema migrations atomically (handles concurrent openers).
        migrations::run_migrations(&mut conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

#[async_trait]
impl BuildRepository for SqliteRepository {
    async fn begin_build(
        &self,
        started_at: &str,
        commit_hash: Option<&str>,
        cargo_args: &str,
        profile: &BuildProfile,
    ) -> anyhow::Result<BuildId> {
        let conn = self.conn.lock().await;

        conn.execute(
            "INSERT INTO builds (started_at, commit_hash, cargo_args, profile) \
             VALUES (?1, ?2, ?3, ?4)",
            (started_at, commit_hash, cargo_args, profile.to_string()),
        )?;

        Ok(BuildId(conn.last_insert_rowid()))
    }

    async fn record_compilation(
        &self,
        build_id: BuildId,
        crate_id: &CrateId,
        kind: &str,
        started_at: &str,
        finished_at: &str,
        duration: Duration,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().await;
        let duration_ms = duration.as_millis() as i64;

        conn.execute(
            "INSERT INTO crate_compilations \
             (build_id, crate_name, crate_version, kind, started_at, finished_at, duration_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            (
                build_id.0,
                &crate_id.name,
                &crate_id.version,
                kind,
                started_at,
                finished_at,
                duration_ms,
            ),
        )?;

        Ok(())
    }

    async fn finalize_build(
        &self,
        build_id: BuildId,
        finished_at: &str,
        success: bool,
        total_duration: Duration,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().await;
        let duration_ms = total_duration.as_millis() as i64;

        conn.execute(
            "UPDATE builds SET finished_at = ?1, success = ?2, total_duration_ms = ?3 \
             WHERE id = ?4",
            (finished_at, success as i32, duration_ms, build_id.0),
        )?;

        Ok(())
    }

    async fn delete_build(&self, id: BuildId) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().await;
        // Atomic: drop the build's crate_compilations and the build row in
        // one transaction so we never end up with orphaned rows.
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM crate_compilations WHERE build_id = ?1", [id.0])?;
        tx.execute("DELETE FROM builds WHERE id = ?1", [id.0])?;
        tx.commit()?;
        Ok(())
    }

    async fn list_builds(&self, limit: usize) -> anyhow::Result<Vec<Build>> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare(
            "SELECT id, started_at, commit_hash, cargo_args, profile, \
                    finished_at, success, total_duration_ms \
             FROM builds ORDER BY started_at DESC LIMIT ?1",
        )?;

        let rows = stmt.query_map([limit as i64], row_to_build)?;

        let mut builds = Vec::new();
        for build in rows {
            builds.push(build?);
        }
        Ok(builds)
    }

    async fn fetch_build(&self, id: BuildId) -> anyhow::Result<Option<BuildDetails>> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare(
            "SELECT id, started_at, commit_hash, cargo_args, profile, \
                    finished_at, success, total_duration_ms \
             FROM builds WHERE id = ?1",
        )?;

        let build = match stmt.query_row([id.0], row_to_build) {
            Ok(b) => b,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        let mut comp_stmt = conn.prepare(
            "SELECT crate_name, crate_version, duration_ms \
             FROM crate_compilations WHERE build_id = ?1",
        )?;

        let comp_rows = comp_stmt.query_map([id.0], |row| {
            let name: String = row.get(0)?;
            let version: Option<String> = row.get(1)?;
            let duration_ms: i64 = row.get(2)?;

            Ok(CrateCompilation {
                crate_id: CrateId { name, version },
                duration: Duration::from_millis(duration_ms as u64),
            })
        })?;

        let mut compilations = Vec::new();
        for c in comp_rows {
            compilations.push(c?);
        }

        Ok(Some(BuildDetails {
            build,
            compilations,
        }))
    }

    async fn fetch_baseline(&self, crate_name: &str) -> anyhow::Result<Option<Baseline>> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare(
            "SELECT COUNT(*), \
                    AVG(duration_ms), \
                    AVG(duration_ms * duration_ms) \
             FROM crate_compilations WHERE crate_name = ?1",
        )?;

        let baseline = stmt.query_row([crate_name], |row| {
            let count: i64 = row.get(0)?;
            if count == 0 {
                return Ok(None);
            }

            let mean_ms: f64 = row.get(1)?;
            let mean_square_ms: f64 = row.get(2)?;

            // Population std dev: sqrt(E[X^2] - (E[X])^2). Clamp negatives from float drift.
            let variance = (mean_square_ms - mean_ms * mean_ms).max(0.0);
            let std_dev_ms = variance.sqrt();

            Ok(Some(Baseline {
                mean: Duration::from_millis(mean_ms as u64),
                std_dev: Duration::from_millis(std_dev_ms as u64),
            }))
        })?;

        Ok(baseline)
    }
}

fn row_to_build(row: &rusqlite::Row) -> rusqlite::Result<Build> {
    let id = BuildId(row.get(0)?);
    let started_at: String = row.get(1)?;
    let commit_hash: Option<String> = row.get(2)?;
    let cargo_args: String = row.get(3)?;
    let profile: String = row.get(4)?;
    let finished_at: Option<String> = row.get(5)?;
    let success_int: Option<i32> = row.get(6)?;
    let duration_ms: Option<i64> = row.get(7)?;

    Ok(Build {
        id,
        started_at,
        finished_at,
        commit_hash,
        cargo_args,
        profile,
        success: success_int.map(|s| s != 0),
        total_duration: duration_ms.map(|d| Duration::from_millis(d as u64)),
    })
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

    /// Two separate SqliteRepository handles on the same DB file must be able
    /// to open and write concurrently without surfacing SQLITE_BUSY. Reproduces
    /// the open-time race documented in issue #3.
    #[tokio::test]
    async fn concurrent_open_and_write_from_two_tasks() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("concurrent.db");
        let p1 = db_path.clone();
        let p2 = db_path.clone();

        let writer = |path: std::path::PathBuf, label: &'static str| async move {
            let repo = SqliteRepository::open(&path).await.unwrap();
            for i in 0..10 {
                let id = repo
                    .begin_build(
                        &format!("2025-01-01T00:00:{:02}Z", i),
                        None,
                        "[]",
                        &BuildProfile::Dev,
                    )
                    .await
                    .unwrap_or_else(|e| panic!("{label} begin_build failed: {e}"));
                repo.record_compilation(
                    id,
                    &lib_crate(label),
                    "lib",
                    "2025-01-01T00:00:00Z",
                    "2025-01-01T00:00:01Z",
                    Duration::from_millis(100),
                )
                .await
                .unwrap_or_else(|e| panic!("{label} record_compilation failed: {e}"));
                repo.finalize_build(id, "2025-01-01T00:00:01Z", true, Duration::from_secs(1))
                    .await
                    .unwrap_or_else(|e| panic!("{label} finalize_build failed: {e}"));
            }
        };

        let h1 = tokio::spawn(writer(p1, "task-a"));
        let h2 = tokio::spawn(writer(p2, "task-b"));
        h1.await.unwrap();
        h2.await.unwrap();

        // Final repo to verify the DB is intact.
        let repo = SqliteRepository::open(&db_path).await.unwrap();
        let builds = repo.list_builds(100).await.unwrap();
        assert_eq!(
            builds.len(),
            20,
            "expected 10 + 10 builds, got {}",
            builds.len()
        );
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
    async fn delete_build_removes_build_and_compilations() {
        let (_d, repo) = fresh_repo().await;
        let id = repo
            .begin_build("2025-01-01T00:00:00Z", None, "[]", &BuildProfile::Dev)
            .await
            .unwrap();
        repo.record_compilation(
            id,
            &lib_crate("foo"),
            "lib",
            "2025-01-01T00:00:00Z",
            "2025-01-01T00:00:01Z",
            Duration::from_millis(100),
        )
        .await
        .unwrap();
        repo.finalize_build(id, "2025-01-01T00:00:01Z", true, Duration::from_secs(1))
            .await
            .unwrap();

        repo.delete_build(id).await.unwrap();

        // Build is gone.
        assert!(repo.fetch_build(id).await.unwrap().is_none());
        // Baseline can no longer find the deleted compilation.
        assert!(repo.fetch_baseline("foo").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_build_is_idempotent_on_missing_id() {
        let (_d, repo) = fresh_repo().await;
        // Deleting a never-inserted id must not error.
        repo.delete_build(BuildId(9999)).await.unwrap();
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

        assert_eq!(baseline.mean, Duration::from_millis(300));

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
