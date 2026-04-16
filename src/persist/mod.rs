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

use crate::model::{
    Baseline, Build, BuildDetails, BuildEvent, BuildId, BuildProfile, CrateId,
};

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
