//! Cargo process supervisor.
//!
//! Spawns `cargo build --message-format=json-render-diagnostics` as a child process
//! and streams its stdout line-by-line through a bounded channel.
//!
//! # Contract
//!
//! - `spawn_build()` returns a `Receiver<String>` that yields one JSON line per message.
//! - The channel is bounded (capacity 1024).
//! - When the cargo process exits, the sender is dropped and the receiver drains remaining lines.
//! - `SupervisorHandle::cancel()` kills the child process.
//! - `SupervisorHandle::wait()` waits for the child to exit and returns its `ExitStatus`.

use std::path::PathBuf;
use std::process::ExitStatus;

use tokio::sync::mpsc;

/// Spawn a cargo build process and return a stream of JSON lines from its stdout.
///
/// # Arguments
///
/// * `cargo_args` — Arguments forwarded to `cargo build`
///   (e.g. `["--release", "-p", "my-crate"]`).
/// * `workspace_dir` — The directory in which to run `cargo build`.
///
/// # Returns
///
/// A tuple of:
/// - `Receiver<String>` — each item is one line of JSON from cargo's stdout.
/// - `SupervisorHandle` — allows cancelling or waiting on the cargo process.
///
/// # Errors
///
/// Returns an error if the cargo process cannot be spawned.
pub async fn spawn_build(
    _cargo_args: Vec<String>,
    _workspace_dir: PathBuf,
) -> anyhow::Result<(mpsc::Receiver<String>, SupervisorHandle)> {
    todo!("Spawn `cargo build --message-format=json-render-diagnostics` and stream stdout lines")
}

/// Handle for a running cargo build process.
///
/// Allows cancelling the build or waiting for it to complete.
pub struct SupervisorHandle {
    // Private fields — will hold the child process handle and cancellation mechanism.
    _private: (),
}

impl SupervisorHandle {
    /// Request cancellation of the cargo build process.
    ///
    /// This kills the child process. The associated `Receiver<String>` will
    /// drain any remaining buffered lines and then close.
    pub fn cancel(&self) {
        todo!("Kill the cargo child process")
    }

    /// Wait for the cargo build process to exit.
    ///
    /// Returns the exit status of the cargo process.
    pub async fn wait(self) -> anyhow::Result<ExitStatus> {
        todo!("Wait for the cargo child process to exit")
    }
}
