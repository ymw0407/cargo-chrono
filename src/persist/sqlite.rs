//! SQLite-backed implementation of `BuildRepository`.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use rusqlite::Connection;
use tokio::sync::Mutex;

use crate::model::{
    Baseline, Build, BuildDetails, BuildId, BuildProfile, CrateId,
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
    use super::*;
    use tempfile::TempDir;

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
}
