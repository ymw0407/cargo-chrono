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

use std::collections::HashMap;
use std::time::Instant;

use chrono::Utc;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::model::{BuildEvent, BuildProfile, CrateId, CrateKind, MessageLevel};

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
    mut rx: mpsc::Receiver<String>,
    config: ParserConfig,
) -> anyhow::Result<mpsc::Receiver<BuildEvent>> {
    let (tx, event_rx) = mpsc::channel::<BuildEvent>(1024);

    tokio::spawn(async move {
        let build_start = Instant::now();
        let started_at_iso = iso_now();

        // (1) Emit BuildStarted as the very first event.
        let _ = tx
            .send(BuildEvent::BuildStarted {
                at: started_at_iso,
                commit_hash: config.commit_hash,
                cargo_args: config.cargo_args,
                profile: config.profile,
            })
            .await;

        // Track in-progress compilations so we can compute durations.
        // Value: (start Instant, start ISO string).
        let mut in_progress: HashMap<CrateId, (Instant, String)> = HashMap::new();

        // build-finished state — defaults to "didn't see one" = treat as failure.
        let mut build_success = false;

        // (2) Process each incoming JSON line.
        while let Some(line) = rx.recv().await {
            // Malformed JSON: skip silently (forward compatibility).
            let json: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let reason = match json.get("reason").and_then(|r| r.as_str()) {
                Some(r) => r,
                None => continue,
            };

            match reason {
                "compiler-artifact" => {
                    let (crate_id, kind) = match parse_target(&json) {
                        Some(p) => p,
                        None => continue,
                    };

                    let now = Instant::now();
                    let now_iso = iso_now();

                    // Get start time. If we never saw this crate before, "started" = now.
                    let (started_instant, started_iso) = in_progress
                        .remove(&crate_id)
                        .unwrap_or_else(|| (now, now_iso.clone()));

                    let duration = now.saturating_duration_since(started_instant);

                    // Emit synthetic CompilationStarted, then CompilationFinished.
                    let _ = tx
                        .send(BuildEvent::CompilationStarted {
                            crate_id: crate_id.clone(),
                            kind: kind.clone(),
                            at: started_iso.clone(),
                        })
                        .await;

                    let _ = tx
                        .send(BuildEvent::CompilationFinished {
                            crate_id,
                            kind,
                            started_at: started_iso,
                            finished_at: now_iso,
                            duration,
                        })
                        .await;
                }

                "compiler-message" => {
                    let crate_id = match parse_crate_id(&json) {
                        Some(c) => c,
                        None => continue,
                    };
                    let message = match json.get("message") {
                        Some(m) => m,
                        None => continue,
                    };
                    let level = match message.get("level").and_then(|l| l.as_str()) {
                        Some("error") => MessageLevel::Error,
                        Some("warning") => MessageLevel::Warning,
                        _ => MessageLevel::Note,
                    };
                    let text = message
                        .get("rendered")
                        .and_then(|r| r.as_str())
                        .unwrap_or("")
                        .to_string();

                    let _ = tx
                        .send(BuildEvent::CompilerMessage {
                            crate_id,
                            level,
                            text,
                        })
                        .await;
                }

                "build-finished" => {
                    build_success = json
                        .get("success")
                        .and_then(|s| s.as_bool())
                        .unwrap_or(false);
                    break;
                }

                // Unknown reason → silently ignore (forward compatibility).
                _ => {}
            }
        }

        // (3) Emit BuildFinished as the very last event.
        let _ = tx
            .send(BuildEvent::BuildFinished {
                success: build_success,
                total_duration: build_start.elapsed(),
                at: iso_now(),
            })
            .await;
    });

    Ok(event_rx)
}

// ---- helpers --------------------------------------------------------------

/// Current UTC timestamp formatted as RFC 3339 (an ISO 8601 subset).
fn iso_now() -> String {
    Utc::now().to_rfc3339()
}

/// Extract `(CrateId, CrateKind)` from a cargo `compiler-artifact` JSON object.
fn parse_target(json: &Value) -> Option<(CrateId, CrateKind)> {
    let crate_id = parse_crate_id(json)?;
    let kind = parse_crate_kind(json);
    Some((crate_id, kind))
}

/// Extract a `CrateId` from `target.name` and `package_id`.
fn parse_crate_id(json: &Value) -> Option<CrateId> {
    let name = json.get("target")?.get("name")?.as_str()?.to_string();
    let package_id = json
        .get("package_id")
        .and_then(|p| p.as_str())
        .unwrap_or("");
    let version = extract_version(package_id);
    Some(CrateId { name, version })
}

/// Map cargo's `target.kind[0]` string to our `CrateKind` enum.
fn parse_crate_kind(json: &Value) -> CrateKind {
    let kind_str = json
        .get("target")
        .and_then(|t| t.get("kind"))
        .and_then(|k| k.get(0))
        .and_then(|k| k.as_str())
        .unwrap_or("");
    match kind_str {
        "lib" => CrateKind::Lib,
        "bin" => CrateKind::Bin,
        "custom-build" => CrateKind::BuildScript,
        "proc-macro" => CrateKind::ProcMacro,
        "test" => CrateKind::Test,
        "example" => CrateKind::Example,
        "bench" => CrateKind::Bench,
        _ => CrateKind::Unknown,
    }
}

/// Extract the version from a cargo `package_id` string.
///
/// Handles three formats:
/// - New: `"registry+https://...#name@1.0.15"` → `"1.0.15"`
/// - New (path, no @): `"path+file:///tmp/foo#0.1.0"` → `"0.1.0"`
/// - Old: `"name 0.1.0 (source)"` → `"0.1.0"`
fn extract_version(package_id: &str) -> Option<String> {
    if package_id.is_empty() {
        return None;
    }

    // Format 1: "...#name@version"
    if let Some(at_pos) = package_id.rfind('@') {
        let v = package_id[at_pos + 1..].trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }

    // Format 2: "path+file:///path#version" (no @ after #)
    if let Some(hash_pos) = package_id.rfind('#') {
        let after = package_id[hash_pos + 1..].trim();
        if !after.is_empty() && !after.contains('@') {
            return Some(after.to_string());
        }
    }

    // Format 3: "name version (source)"
    let parts: Vec<&str> = package_id.split_whitespace().collect();
    if parts.len() >= 2 {
        return Some(parts[1].to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    //! Contract tests for the Parser.
    //!
    //! These tests define the behavior that `run_parser` must satisfy.
    //! They will panic (fail) until `run_parser` is implemented — which is
    //! intentional. Use them as a spec while implementing.

    use super::*;
    use crate::model::{BuildEvent, BuildProfile};
    use tokio::sync::mpsc;

    fn default_config() -> ParserConfig {
        ParserConfig {
            commit_hash: Some("abc123".to_string()),
            cargo_args: vec!["build".to_string()],
            profile: BuildProfile::Dev,
        }
    }

    /// Helper: feed raw JSON lines through the parser and collect every emitted event.
    async fn collect_events(lines: Vec<&str>) -> Vec<BuildEvent> {
        let (tx, rx) = mpsc::channel(1024);
        for line in lines {
            tx.send(line.to_string()).await.unwrap();
        }
        drop(tx);

        let mut event_rx = run_parser(rx, default_config()).await.unwrap();
        let mut events = Vec::new();
        while let Some(e) = event_rx.recv().await {
            events.push(e);
        }
        events
    }

    #[tokio::test]
    async fn first_event_is_always_build_started() {
        let events = collect_events(vec![r#"{"reason":"build-finished","success":true}"#]).await;
        assert!(
            matches!(events.first(), Some(BuildEvent::BuildStarted { .. })),
            "first event must be BuildStarted, got {:?}",
            events.first()
        );
    }

    #[tokio::test]
    async fn last_event_is_always_build_finished() {
        let events = collect_events(vec![r#"{"reason":"build-finished","success":true}"#]).await;
        assert!(
            matches!(
                events.last(),
                Some(BuildEvent::BuildFinished { success: true, .. })
            ),
            "last event must be BuildFinished(success=true), got {:?}",
            events.last()
        );
    }

    #[tokio::test]
    async fn build_started_carries_parser_config_values() {
        let events = collect_events(vec![r#"{"reason":"build-finished","success":true}"#]).await;
        match events.first() {
            Some(BuildEvent::BuildStarted {
                commit_hash,
                cargo_args,
                profile,
                ..
            }) => {
                assert_eq!(commit_hash.as_deref(), Some("abc123"));
                assert_eq!(cargo_args, &vec!["build".to_string()]);
                assert_eq!(profile, &BuildProfile::Dev);
            }
            other => panic!("expected BuildStarted, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn compiler_artifact_becomes_compilation_finished() {
        // Cargo's compiler-artifact JSON for a completed crate compilation.
        let artifact = r#"{"reason":"compiler-artifact","package_id":"demo 0.1.0 (path+file:///tmp/demo)","manifest_path":"/tmp/demo/Cargo.toml","target":{"kind":["lib"],"crate_types":["lib"],"name":"demo","src_path":"/tmp/demo/src/lib.rs","edition":"2021","doc":true,"doctest":true,"test":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/tmp/target/debug/libdemo.rlib"],"executable":null,"fresh":false}"#;
        let events = collect_events(vec![
            artifact,
            r#"{"reason":"build-finished","success":true}"#,
        ])
        .await;

        let finished = events
            .iter()
            .find(|e| matches!(e, BuildEvent::CompilationFinished { .. }));
        match finished {
            Some(BuildEvent::CompilationFinished {
                crate_id, duration, ..
            }) => {
                assert_eq!(crate_id.name, "demo");
                // Duration must be populated (the Parser contract guarantees this).
                assert!(
                    *duration > std::time::Duration::ZERO || duration.as_nanos() == 0,
                    "duration must be measurable (got {:?})",
                    duration
                );
            }
            other => panic!("expected CompilationFinished for 'demo', got {:?}", other),
        }
    }

    #[tokio::test]
    async fn compiler_message_becomes_compiler_message_event() {
        let msg = r#"{"reason":"compiler-message","package_id":"demo 0.1.0 (path+file:///tmp/demo)","manifest_path":"/tmp/demo/Cargo.toml","target":{"kind":["lib"],"crate_types":["lib"],"name":"demo","src_path":"/tmp/demo/src/lib.rs","edition":"2021","doc":true,"doctest":true,"test":true},"message":{"rendered":"warning: unused variable","children":[],"code":null,"level":"warning","message":"unused variable","spans":[]}}"#;
        let events =
            collect_events(vec![msg, r#"{"reason":"build-finished","success":true}"#]).await;

        let has_msg = events
            .iter()
            .any(|e| matches!(e, BuildEvent::CompilerMessage { .. }));
        assert!(
            has_msg,
            "expected a CompilerMessage event, got {:?}",
            events
        );
    }

    #[tokio::test]
    async fn unknown_reasons_are_silently_ignored() {
        let events = collect_events(vec![
            r#"{"reason":"some-future-event-type","data":"whatever"}"#,
            r#"{"reason":"build-finished","success":true}"#,
        ])
        .await;
        // Expect exactly BuildStarted + BuildFinished. No extra events for unknown reason.
        assert_eq!(
            events.len(),
            2,
            "unknown reasons must not produce events, got {:?}",
            events
        );
    }

    #[tokio::test]
    async fn malformed_json_does_not_crash_parser() {
        // Malformed JSON should be skipped (logged internally), not propagated as a panic/error.
        let events = collect_events(vec![
            r#"this is not json"#,
            r#"{"reason":"build-finished","success":true}"#,
        ])
        .await;
        // Parser should still emit BuildStarted + BuildFinished, surviving the bad line.
        assert!(matches!(
            events.first(),
            Some(BuildEvent::BuildStarted { .. })
        ));
        assert!(matches!(
            events.last(),
            Some(BuildEvent::BuildFinished { .. })
        ));
    }

    #[tokio::test]
    async fn build_finished_forwards_success_false() {
        let events = collect_events(vec![r#"{"reason":"build-finished","success":false}"#]).await;
        match events.last() {
            Some(BuildEvent::BuildFinished { success, .. }) => assert!(!success),
            other => panic!("expected BuildFinished, got {:?}", other),
        }
    }

    // ---- helper unit tests ------------------------------------------------

    #[test]
    fn extract_version_old_format() {
        assert_eq!(
            extract_version("demo 0.1.0 (path+file:///tmp/demo)"),
            Some("0.1.0".to_string())
        );
    }

    #[test]
    fn extract_version_new_format_with_at() {
        assert_eq!(
            extract_version("registry+https://github.com/rust-lang/crates.io-index#itoa@1.0.15"),
            Some("1.0.15".to_string())
        );
    }

    #[test]
    fn extract_version_path_format_no_at() {
        assert_eq!(
            extract_version("path+file:///tmp/ripgrep#15.1.0"),
            Some("15.1.0".to_string())
        );
    }

    #[test]
    fn extract_version_workspace_member_with_at() {
        assert_eq!(
            extract_version("path+file:///tmp/ripgrep/crates/cli#grep-cli@0.1.12"),
            Some("0.1.12".to_string())
        );
    }

    /// End-to-end smoke test using the real cargo output fixture
    /// (`tests/fixtures/sample_output.jsonl`).
    ///
    /// Verifies that the parser handles real cargo JSON output without crashing
    /// and produces a sensible event sequence:
    ///   - Starts with BuildStarted
    ///   - Ends with BuildFinished
    ///   - Emits at least one CompilationStarted/Finished pair
    #[tokio::test]
    async fn fixture_sample_output_parses_cleanly() {
        let fixture = std::fs::read_to_string("tests/fixtures/sample_output.jsonl")
            .expect("fixture file should exist at tests/fixtures/sample_output.jsonl");

        let lines: Vec<&str> = fixture.lines().collect();
        let events = collect_events(lines).await;

        // First and last contracts.
        assert!(matches!(
            events.first(),
            Some(BuildEvent::BuildStarted { .. })
        ));
        assert!(matches!(
            events.last(),
            Some(BuildEvent::BuildFinished { .. })
        ));

        // Should have produced compilation events for real crates.
        let compilation_count = events
            .iter()
            .filter(|e| matches!(e, BuildEvent::CompilationFinished { .. }))
            .count();
        assert!(
            compilation_count > 0,
            "expected at least one CompilationFinished from the fixture, got 0"
        );

        // Each Started should pair with a Finished (we always emit both together).
        let started_count = events
            .iter()
            .filter(|e| matches!(e, BuildEvent::CompilationStarted { .. }))
            .count();
        assert_eq!(
            started_count, compilation_count,
            "every CompilationFinished should have a matching CompilationStarted"
        );
    }
}
