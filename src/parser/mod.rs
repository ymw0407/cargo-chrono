//! Cargo JSON output parser.
//!
//! Transforms raw JSON lines from `cargo build --message-format=json-render-diagnostics`
//! into a typed `BuildEvent` stream.
//!
//! # Contract
//!
//! - The first event emitted is always `BuildStarted`.
//! - The last event emitted is always `BuildFinished`.
//! - `CompilationFinished` events always contain a valid `duration`, `started_at`,
//!   and `finished_at`. The Parser internally matches `CompilationStarted` /
//!   `CompilationFinished` pairs to compute durations.
//! - The output channel is bounded (capacity 1024).
//! - Unknown JSON messages are silently ignored (forward compatibility).
//!
//! # Implementation notes
//!
//! - Uses `serde_json` for direct parsing (no `cargo_metadata` crate).
//! - Maintains an internal `HashMap<CrateId, Instant>` to match start/finish pairs.

use tokio::sync::mpsc;

use crate::model::{BuildEvent, BuildProfile};

/// Configuration for the parser.
pub struct ParserConfig {
    /// Git commit hash at the time of the build (from `git rev-parse HEAD`).
    pub commit_hash: Option<String>,
    /// The cargo arguments used for this build (for recording purposes).
    pub cargo_args: Vec<String>,
    /// The build profile.
    pub profile: BuildProfile,
}

/// Run the parser, consuming raw JSON lines and producing `BuildEvent`s.
///
/// # Arguments
///
/// * `rx` — Receiver of raw JSON lines from the supervisor.
/// * `config` — Parser configuration (commit hash, cargo args, profile).
///
/// # Returns
///
/// A `Receiver<BuildEvent>` that yields parsed build events in order.
/// The first event is `BuildStarted` and the last is `BuildFinished`.
///
/// # Errors
///
/// Returns an error if the parser encounters a fatal condition
/// (e.g., the input channel closes before any events are received).
pub async fn run_parser(
    _rx: mpsc::Receiver<String>,
    _config: ParserConfig,
) -> anyhow::Result<mpsc::Receiver<BuildEvent>> {
    todo!("Parse JSON lines into BuildEvent stream")
}
