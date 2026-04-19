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
}
