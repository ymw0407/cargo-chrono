//! Build data persistence layer.
//!
//! Provides the `BuildRepository` trait for storing and retrieving build data,
//! and a SQLite-backed implementation.
//!
//! Owned by the Data team. The Realtime team may use `BuildRepository` (the trait)
//! but must not import `SqliteRepository` directly.

mod migrations;
mod sqlite;

pub use sqlite::SqliteRepository;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::model::{Baseline, Build, BuildDetails, BuildEvent, BuildId, BuildProfile, CrateId};

/// Abstract repository for build data.
///
/// All methods are async to allow for future non-blocking implementations.
/// Implementations must be `Send + Sync` for use with `Arc<dyn BuildRepository>`.
#[async_trait]
pub trait BuildRepository: Send + Sync {
    /// Create a new build record and return its ID.
    ///
    /// Called when a `BuildStarted` event is received.
    ///
    /// # Arguments
    /// * `started_at` — ISO 8601 timestamp
    /// * `commit_hash` — Git commit hash, if available
    /// * `cargo_args` — JSON-serialized cargo arguments
    /// * `profile` — Build profile
    async fn begin_build(
        &self,
        started_at: &str,
        commit_hash: Option<&str>,
        cargo_args: &str,
        profile: &BuildProfile,
    ) -> anyhow::Result<BuildId>;

    /// Record a single crate compilation.
    ///
    /// Called when a `CompilationFinished` event is received.
    async fn record_compilation(
        &self,
        build_id: BuildId,
        crate_id: &CrateId,
        kind: &str,
        started_at: &str,
        finished_at: &str,
        duration: Duration,
    ) -> anyhow::Result<()>;

    /// Finalize a build record with completion data.
    ///
    /// Called when a `BuildFinished` event is received.
    async fn finalize_build(
        &self,
        build_id: BuildId,
        finished_at: &str,
        success: bool,
        total_duration: Duration,
    ) -> anyhow::Result<()>;

    /// List the most recent builds.
    ///
    /// Returns up to `limit` builds, ordered by start time descending.
    async fn list_builds(&self, limit: usize) -> anyhow::Result<Vec<Build>>;

    /// Fetch a single build with all its compilation details.
    ///
    /// Returns `None` if the build ID does not exist.
    async fn fetch_build(&self, id: BuildId) -> anyhow::Result<Option<BuildDetails>>;

    /// Fetch the statistical baseline for a crate.
    ///
    /// Computes mean, standard deviation, min, and max from historical compilation times.
    /// Returns `None` if there is insufficient data.
    async fn fetch_baseline(&self, crate_name: &str) -> anyhow::Result<Option<Baseline>>;
}

/// Consume a `BuildEvent` stream and persist each event to the repository.
///
/// # Contract
///
/// - Expects the first event to be `BuildStarted` (calls `begin_build`).
/// - Records each `CompilationFinished` event.
/// - Finalizes the build on `BuildFinished`.
/// - Returns the `BuildId` of the recorded build.
///
/// # Errors
///
/// Returns an error if any database operation fails or if the stream
/// does not start with `BuildStarted`.
pub async fn run_persister(
    _repo: Arc<dyn BuildRepository>,
    _rx: mpsc::Receiver<BuildEvent>,
) -> anyhow::Result<BuildId> {
    todo!("Consume BuildEvent stream and persist to repository")
}

#[cfg(test)]
mod tests {
    //! Contract tests for `run_persister`.

    use super::*;
    use crate::model::{CrateId, CrateKind};
    use tempfile::TempDir;

    async fn setup() -> (TempDir, Arc<dyn BuildRepository>) {
        let dir = TempDir::new().unwrap();
        let repo = SqliteRepository::open(&dir.path().join("test.db"))
            .await
            .unwrap();
        (dir, Arc::new(repo))
    }

    fn build_started() -> BuildEvent {
        BuildEvent::BuildStarted {
            at: "2025-01-01T00:00:00Z".into(),
            commit_hash: Some("abc123".into()),
            cargo_args: vec!["build".into()],
            profile: BuildProfile::Dev,
        }
    }

    fn compilation_finished(name: &str, ms: u64) -> BuildEvent {
        BuildEvent::CompilationFinished {
            crate_id: CrateId {
                name: name.into(),
                version: None,
            },
            kind: CrateKind::Lib,
            started_at: "2025-01-01T00:00:00Z".into(),
            finished_at: "2025-01-01T00:00:01Z".into(),
            duration: Duration::from_millis(ms),
        }
    }

    fn build_finished(success: bool, total_ms: u64) -> BuildEvent {
        BuildEvent::BuildFinished {
            success,
            total_duration: Duration::from_millis(total_ms),
            at: "2025-01-01T00:00:01Z".into(),
        }
    }

    #[tokio::test]
    async fn persists_full_build_and_returns_build_id() {
        let (_d, repo) = setup().await;
        let (tx, rx) = mpsc::channel(16);

        tx.send(build_started()).await.unwrap();
        tx.send(compilation_finished("foo", 1000)).await.unwrap();
        tx.send(compilation_finished("bar", 500)).await.unwrap();
        tx.send(build_finished(true, 1500)).await.unwrap();
        drop(tx);

        let id = run_persister(repo.clone(), rx).await.unwrap();
        let details = repo.fetch_build(id).await.unwrap().expect("build recorded");
        assert_eq!(details.compilations.len(), 2);
        assert_eq!(details.build.success, Some(true));
        assert_eq!(
            details.build.total_duration,
            Some(Duration::from_millis(1500))
        );
    }

    #[tokio::test]
    async fn ignores_non_persisted_event_kinds() {
        // CompilationStarted and CompilerMessage are informational — they should
        // be consumed without error and without adding database rows.
        let (_d, repo) = setup().await;
        let (tx, rx) = mpsc::channel(16);

        tx.send(build_started()).await.unwrap();
        tx.send(BuildEvent::CompilationStarted {
            crate_id: CrateId {
                name: "foo".into(),
                version: None,
            },
            kind: CrateKind::Lib,
            at: "2025-01-01T00:00:00Z".into(),
        })
        .await
        .unwrap();
        tx.send(BuildEvent::CompilerMessage {
            crate_id: CrateId {
                name: "foo".into(),
                version: None,
            },
            level: crate::model::MessageLevel::Warning,
            text: "unused variable".into(),
        })
        .await
        .unwrap();
        tx.send(build_finished(true, 100)).await.unwrap();
        drop(tx);

        let id = run_persister(repo.clone(), rx).await.unwrap();
        let details = repo.fetch_build(id).await.unwrap().unwrap();
        assert!(details.compilations.is_empty());
    }

    #[tokio::test]
    async fn errors_if_first_event_is_not_build_started() {
        let (_d, repo) = setup().await;
        let (tx, rx) = mpsc::channel(16);

        tx.send(build_finished(true, 0)).await.unwrap();
        drop(tx);

        let result = run_persister(repo, rx).await;
        assert!(
            result.is_err(),
            "expected error when first event is not BuildStarted, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn records_failed_build() {
        let (_d, repo) = setup().await;
        let (tx, rx) = mpsc::channel(16);

        tx.send(build_started()).await.unwrap();
        tx.send(build_finished(false, 200)).await.unwrap();
        drop(tx);

        let id = run_persister(repo.clone(), rx).await.unwrap();
        let details = repo.fetch_build(id).await.unwrap().unwrap();
        assert_eq!(details.build.success, Some(false));
    }
}
