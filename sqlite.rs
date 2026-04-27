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
pub struct SqliteRepository {
    conn: Mutex<Connection>,
}

impl SqliteRepository {
    /// Open (or create) a SQLite database at the given path.
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
        started_at: &str,
        commit_hash: Option<&str>,
        cargo_args: &str,
        profile: &BuildProfile,
    ) -> anyhow::Result<BuildId> {
        let conn = self.conn.lock().await;

        let profile_str = match profile {
            BuildProfile::Dev => "dev",
            BuildProfile::Release => "release",
        };

        conn.execute(
            "INSERT INTO builds (started_at, commit_hash, cargo_args, profile) VALUES (?1, ?2, ?3, ?4)",
            (started_at, commit_hash, cargo_args, profile_str),
        )?;

        let last_id = conn.last_insert_rowid();
        Ok(BuildId(last_id))
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
            "INSERT INTO crate_compilations (build_id, crate_name, crate_version, kind, started_at, finished_at, duration_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
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
        let success_int = if success { 1 } else { 0 };

        conn.execute(
            "UPDATE builds SET finished_at = ?1, success = ?2, total_duration_ms = ?3 WHERE id = ?4",
            (finished_at, success_int, duration_ms, build_id.0),
        )?;

        Ok(())
    }

    async fn list_builds(&self, limit: usize) -> anyhow::Result<Vec<Build>> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare(
            "SELECT id, started_at, finished_at, success, total_duration_ms FROM builds ORDER BY started_at DESC LIMIT ?1"
        )?;

        let rows = stmt.query_map([limit], |row| {
            let id = BuildId(row.get(0)?);
            let started_at: String = row.get(1)?;
            let finished_at: Option<String> = row.get(2)?;
            let success_int: Option<i32> = row.get(3)?;
            let duration_ms: Option<i64> = row.get(4)?;

            let success = success_int.map(|s| s != 0);
            let total_duration = duration_ms.map(|d| Duration::from_millis(d as u64));

            Ok(Build {
                id,
                started_at,
                finished_at,
                success,
                total_duration,
            })
        })?;

        let mut builds = Vec::new();
        for build in rows {
            builds.push(build?);
        }

        Ok(builds)
    }

    async fn fetch_build(&self, id: BuildId) -> anyhow::Result<Option<BuildDetails>> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare(
            "SELECT id, started_at, commit_hash, cargo_args, profile, finished_at, success, total_duration_ms FROM builds WHERE id = ?1"
        )?;

        let build_opt = stmt.query_row([id.0], |row| {
            let build_id = BuildId(row.get(0)?);
            let started_at: String = row.get(1)?;
            let commit_hash: Option<String> = row.get(2)?;
            let cargo_args: String = row.get(3)?;
            let profile_str: String = row.get(4)?;
            let finished_at: Option<String> = row.get(5)?;
            let success_int: Option<i32> = row.get(6)?;
            let duration_ms: Option<i64> = row.get(7)?;

            let profile = match profile_str.as_str() {
                "release" => BuildProfile::Release,
                _ => BuildProfile::Dev,
            };

            let success = success_int.map(|s| s != 0);
            let total_duration = duration_ms.map(|d| Duration::from_millis(d as u64));

            Ok(Build {
                id: build_id,
                started_at,
                finished_at,
                success,
                total_duration,
            })
        }).ok(); // Returns None if no row found

        if let Some(build) = build_opt {
            // Fetch compilations
            let mut stmt_comp = conn.prepare(
                "SELECT crate_name, crate_version, kind, started_at, finished_at, duration_ms FROM crate_compilations WHERE build_id = ?1"
            )?;

            let compilations_rows = stmt_comp.query_map([id.0], |row| {
                let name: String = row.get(0)?;
                let version: Option<String> = row.get(1)?;
                let kind: String = row.get(2)?;
                let started_at: String = row.get(3)?;
                let finished_at: String = row.get(4)?;
                let duration_ms: i64 = row.get(5)?;

                Ok(crate::model::CrateCompilation {
                    crate_id: CrateId { name, version },
                    kind,
                    started_at,
                    finished_at,
                    duration: Duration::from_millis(duration_ms as u64),
                })
            })?;

            let mut compilations = Vec::new();
            for comp in compilations_rows {
                compilations.push(comp?);
            }

            Ok(Some(BuildDetails {
                build,
                compilations,
            }))
        } else {
            Ok(None)
        }
    }

    async fn fetch_baseline(&self, crate_name: &str) -> anyhow::Result<Option<Baseline>> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare(
            "SELECT COUNT(*), AVG(duration_ms), MIN(duration_ms), MAX(duration_ms), AVG(duration_ms * duration_ms) FROM crate_compilations WHERE crate_name = ?1"
        )?;

        let baseline_opt = stmt.query_row([crate_name], |row| {
            let count: i64 = row.get(0)?;
            if count == 0 {
                return Ok(None);
            }

            let mean_ms: f64 = row.get(1)?;
            let min_ms: i64 = row.get(2)?;
            let max_ms: i64 = row.get(3)?;
            let mean_square_ms: f64 = row.get(4)?;

            // Calculate Standard Deviation: sqrt(E[X^2] - (E[X])^2)
            let variance = mean_square_ms - (mean_ms * mean_ms);
            let std_dev_ms = if variance > 0.0 { variance.sqrt() } else { 0.0 };

            Ok(Some(Baseline {
                sample_count: count as usize,
                mean: Duration::from_millis(mean_ms as u64),
                std_dev: Duration::from_millis(std_dev_ms as u64),
                min: Duration::from_millis(min_ms as u64),
                max: Duration::from_millis(max_ms as u64),
            }))
        })?;

        Ok(baseline_opt)
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
