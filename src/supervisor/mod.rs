//! Cargo process supervisor.
//!
//! Spawns `cargo build --message-format=json-render-diagnostics` as a child process
//! and streams **both** its stdout (JSON) and stderr (`Compiling foo v0.1.0`
//! progress lines) line-by-line through a single bounded channel.
//!
//! Merging the two streams keeps the parser interface as a single
//! `Receiver<String>`. Cargo's stderr is plain text and stdout is JSON-per-line,
//! so the parser can disambiguate cheaply (JSON lines start with `{`).
//!
//! # Contract
//!
//! - `spawn_build()` returns a `Receiver<String>` that yields one line per
//!   message — stdout and stderr lines may interleave.
//! - The channel is bounded (capacity 1024).
//! - The channel closes only after **both** stdout and stderr reach EOF.
//! - `SupervisorHandle::cancel()` kills the child process.
//! - `SupervisorHandle::wait()` waits for the child to exit and returns its `ExitStatus`.

use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::Mutex;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

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
    cargo_args: Vec<String>,
    workspace_dir: PathBuf,
) -> anyhow::Result<(mpsc::Receiver<String>, SupervisorHandle)> {
    // Built-in cargo args = "build" plus the JSON output format flag.
    // User-supplied args are appended after (e.g. "--release", "-p name").
    let mut args = vec![
        "build".to_string(),
        "--message-format=json-render-diagnostics".to_string(),
    ];
    args.extend(cargo_args);

    spawn_program("cargo", args, workspace_dir).await
}

/// Spawn an arbitrary external program and expose its stdout as a stream of lines.
///
/// This is the generalised internal helper behind `spawn_build`. Factoring it
/// out lets unit tests exercise the spawn / stream / cancel paths against a
/// lightweight command (e.g. `sh -c "..."`) instead of requiring a real cargo
/// project.
async fn spawn_program(
    program: &str,
    args: Vec<String>,
    workspace_dir: PathBuf,
) -> anyhow::Result<(mpsc::Receiver<String>, SupervisorHandle)> {
    // ── Spawn the child process ─────────────────────────────────────
    // `tokio::process::Command` is the async wrapper around `std::process::Command`.
    //   stdout(Stdio::piped()) — pipe the child's stdout (JSON messages).
    //   stderr(Stdio::piped()) — pipe the child's stderr (cargo's
    //                            "Compiling foo v0.1.0" progress lines, which
    //                            the parser uses as compilation-start anchors).
    //   kill_on_drop(true)     — automatically SIGKILL the child if the `Child`
    //                            handle is dropped. Defence in depth.
    let mut child = Command::new(program)
        .args(&args)
        .current_dir(&workspace_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("child stdout was not piped"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("child stderr was not piped"))?;

    // ── Output channel ─────────────────────────────────────────────
    // One line == one message. Capacity 1024 — when the buffer fills up, the
    // reader task's `tx.send().await` will park, naturally backpressuring the
    // child until the consumer catches up.
    let (tx, rx) = mpsc::channel::<String>(1024);

    // ── Cancellation token ─────────────────────────────────────────
    // When `SupervisorHandle::cancel()` is called, this token fires and the
    // reader tasks exit their loops. The child process itself is killed
    // separately via `start_kill()` (see `SupervisorHandle::cancel`).
    let cancel = CancellationToken::new();

    // ── Reader tasks ───────────────────────────────────────────────
    // Two readers, one per pipe, share `tx` (via clone). The mpsc channel
    // closes only when *both* tx clones are dropped, i.e. when both EOF.
    spawn_line_reader(stdout, tx.clone(), cancel.clone());
    spawn_line_reader(stderr, tx, cancel.clone());

    let handle = SupervisorHandle {
        child: Mutex::new(Some(child)),
        cancel,
    };

    Ok((rx, handle))
}

/// Spawn a task that reads `pipe` line by line and forwards each line to `tx`,
/// stopping on cancel, EOF, IO error, or receiver drop.
fn spawn_line_reader<R>(pipe: R, tx: mpsc::Sender<String>, cancel: CancellationToken)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(pipe).lines();
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => break,
                next = lines.next_line() => {
                    match next {
                        Ok(Some(line)) => {
                            if tx.send(line).await.is_err() {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
            }
        }
    });
}

/// Handle for a running cargo build process.
///
/// Allows cancelling the build or waiting for it to complete.
pub struct SupervisorHandle {
    /// The child process. We use interior mutability (`std::sync::Mutex`)
    /// because `cancel(&self)` only takes a shared reference. The `Option`
    /// is here because `wait()` consumes the child via `take()`.
    child: Mutex<Option<Child>>,
    /// Token signalling the reader task to stop. Fired by `cancel()`.
    cancel: CancellationToken,
}

impl SupervisorHandle {
    /// Request cancellation of the cargo build process.
    ///
    /// This kills the child process. The associated `Receiver<String>` will
    /// drain any remaining buffered lines and then close.
    ///
    /// Idempotent: safe to call multiple times. Subsequent calls are essentially no-ops.
    pub fn cancel(&self) {
        // Signal the reader task — it exits at the next await point.
        self.cancel.cancel();

        // Send SIGKILL to the child process.
        // `start_kill` is synchronous (no await needed), so it's safe to call
        // from `cancel(&self)`. A locking failure here means `wait()` already
        // took the child out — treat as a no-op.
        if let Ok(mut guard) = self.child.lock() {
            if let Some(child) = guard.as_mut() {
                let _ = child.start_kill();
            }
        }
    }

    /// Wait for the cargo build process to exit.
    ///
    /// Returns the exit status of the cargo process. Consumes `self`, so it
    /// can only be called once.
    pub async fn wait(self) -> anyhow::Result<ExitStatus> {
        // Take the child out of the mutex. The lock guard must be dropped
        // immediately so a concurrent `cancel()` call can still acquire the
        // mutex while we're awaiting the child below.
        let mut child = {
            let mut guard = self
                .child
                .lock()
                .map_err(|_| anyhow::anyhow!("child mutex poisoned"))?;
            guard
                .take()
                .ok_or_else(|| anyhow::anyhow!("child already taken"))?
        };
        Ok(child.wait().await?)
    }
}

#[cfg(test)]
mod tests {
    //! Tests use `sh -c "..."` to spawn lightweight subprocesses instead of
    //! a real cargo build. The same spawn / stream / cancel logic is
    //! exercised, but the tests stay fast and deterministic.

    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    /// Helper: spawn `sh -c <cmd>` with cwd = /tmp.
    async fn spawn_sh(command: &str) -> anyhow::Result<(mpsc::Receiver<String>, SupervisorHandle)> {
        spawn_program(
            "sh",
            vec!["-c".to_string(), command.to_string()],
            PathBuf::from("/tmp"),
        )
        .await
    }

    #[tokio::test]
    async fn streams_stdout_lines_in_order() {
        let (mut rx, handle) = spawn_sh("printf 'line1\\nline2\\nline3\\n'").await.unwrap();

        let l1 = rx.recv().await.unwrap();
        let l2 = rx.recv().await.unwrap();
        let l3 = rx.recv().await.unwrap();
        assert_eq!(l1, "line1");
        assert_eq!(l2, "line2");
        assert_eq!(l3, "line3");

        let status = handle.wait().await.unwrap();
        assert!(status.success(), "exit was {:?}", status);
    }

    #[tokio::test]
    async fn channel_closes_when_child_exits() {
        // After the child closes stdout and exits, the receiver should see None.
        let (mut rx, handle) = spawn_sh("printf 'only\\n'").await.unwrap();
        let _ = rx.recv().await.unwrap();
        let next = timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("recv did not return within 2s");
        assert!(next.is_none(), "expected channel close, got {:?}", next);
        let _ = handle.wait().await;
    }

    #[tokio::test]
    async fn cancel_kills_long_running_process() {
        // A 60s sleep — must terminate promptly after cancel.
        let (_rx, handle) = spawn_sh("sleep 60").await.unwrap();

        handle.cancel();

        let status = timeout(Duration::from_secs(3), handle.wait())
            .await
            .expect("wait did not return within 3s after cancel")
            .unwrap();
        // Killed by SIGKILL — not a clean exit.
        assert!(!status.success(), "process should have been killed");
    }

    #[tokio::test]
    async fn cancel_is_idempotent() {
        let (_rx, handle) = spawn_sh("sleep 60").await.unwrap();
        handle.cancel();
        handle.cancel(); // second call must not panic
        let _ = timeout(Duration::from_secs(3), handle.wait()).await;
    }

    #[tokio::test]
    async fn missing_program_returns_error() {
        let result = spawn_program(
            "this-program-does-not-exist-xyz",
            vec![],
            PathBuf::from("/tmp"),
        )
        .await;
        assert!(result.is_err(), "expected spawn to fail");
    }

    #[tokio::test]
    async fn dropping_receiver_does_not_panic_reader_task() {
        // Dropping the receiver immediately must let the reader task exit
        // cleanly without panicking.
        let (rx, handle) = spawn_sh("printf 'a\\nb\\nc\\n' && sleep 0.1")
            .await
            .unwrap();
        drop(rx);
        // Wait briefly for the child to exit. No panics should occur.
        let _ = timeout(Duration::from_secs(2), handle.wait()).await;
    }

    /// End-to-end check that real cargo output flows through the supervisor.
    ///
    /// Slow (~a few seconds for a hello-world build), so it's `#[ignore]`d
    /// by default. Run explicitly with:
    ///
    /// ```text
    /// cargo test -- --ignored real_cargo_build
    /// ```
    #[tokio::test]
    #[ignore]
    async fn real_cargo_build_streams_json_lines() {
        // 1. Create a tiny hello-world cargo project in a temp directory.
        let dir = tempfile::tempdir().expect("tempdir");
        let workspace = dir.path().to_path_buf();
        std::fs::write(
            workspace.join("Cargo.toml"),
            r#"[package]
name = "demo_chrono"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "demo_chrono"
path = "src/main.rs"
"#,
        )
        .unwrap();
        std::fs::create_dir_all(workspace.join("src")).unwrap();
        std::fs::write(
            workspace.join("src/main.rs"),
            "fn main() { println!(\"hi\"); }\n",
        )
        .unwrap();

        // 2. Spawn cargo via the supervisor.
        let (mut rx, handle) = spawn_build(vec![], workspace).await.unwrap();

        // 3. Drain and classify the lines.
        let mut total = 0usize;
        let mut compiler_artifact_count = 0usize;
        let mut build_finished_count = 0usize;
        let mut first_line: Option<String> = None;
        while let Some(line) = rx.recv().await {
            total += 1;
            if first_line.is_none() {
                first_line = Some(line.clone());
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                match v.get("reason").and_then(|r| r.as_str()) {
                    Some("compiler-artifact") => compiler_artifact_count += 1,
                    Some("build-finished") => build_finished_count += 1,
                    _ => {}
                }
            }
        }

        let status = handle.wait().await.unwrap();
        eprintln!(
            "real cargo build: total={total}, artifact={compiler_artifact_count}, \
             build_finished={build_finished_count}, exit={:?}",
            status
        );
        if let Some(line) = &first_line {
            let preview: String = line.chars().take(120).collect();
            eprintln!("first line: {preview}...");
        }

        // 4. Assertions.
        assert!(status.success(), "cargo build failed: {:?}", status);
        assert!(total > 0, "expected at least one line");
        assert!(
            compiler_artifact_count >= 1,
            "expected at least one compiler-artifact, got {compiler_artifact_count}"
        );
        assert_eq!(
            build_finished_count, 1,
            "expected exactly one build-finished, got {build_finished_count}"
        );
    }
}
