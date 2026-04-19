//! CLI definition and output rendering.
//!
//! Owned by the Integrator. Uses clap 4 derive macros for argument parsing.

use clap::{Parser, Subcommand};

use crate::model::{Build, BuildDiff, BuildId, CrateChange, DurationChange};

/// cargo-chrono — Cargo build performance observer.
#[derive(Parser, Debug)]
#[command(name = "cargo-chrono", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Available subcommands.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run a build and record it to the local database.
    Record {
        /// Arguments forwarded to `cargo build`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        cargo_args: Vec<String>,
    },

    /// Run a build with a real-time TUI dashboard.
    Watch {
        /// Arguments forwarded to `cargo build`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        cargo_args: Vec<String>,
    },

    /// List recent builds from the database.
    Ls {
        /// Number of most recent builds to display.
        #[arg(long, default_value = "10")]
        last: usize,
    },

    /// Compare two recorded builds.
    Diff {
        /// Build ID of the "before" build.
        before: i64,
        /// Build ID of the "after" build.
        after: i64,
    },
}

/// Render a list of builds to stdout.
///
/// Displays a simple table with build ID, timestamp, profile, duration, and status.
pub fn render_ls(builds: &[Build]) {
    if builds.is_empty() {
        println!("No builds recorded yet.");
        return;
    }

    let header = format!(
        "{:<6} {:<20} {:<8} {:<10} {}",
        "ID", "Started", "Profile", "Duration", "Status"
    );
    println!("{header}");
    println!("{}", "-".repeat(60));

    for build in builds {
        let duration_str = match build.total_duration {
            Some(d) => format!("{:.1}s", d.as_secs_f64()),
            None => "—".to_string(),
        };
        let status = match build.success {
            Some(true) => "ok",
            Some(false) => "FAIL",
            None => "???",
        };
        println!(
            "{:<6} {:<20} {:<8} {:<10} {}",
            BuildId(build.id.0),
            &build.started_at[..std::cmp::min(19, build.started_at.len())],
            build.profile,
            duration_str,
            status,
        );
    }
}

/// Render a build diff to stdout.
///
/// Shows total duration change, per-crate changes (sorted by impact),
/// and critical path comparison.
pub fn render_diff(diff: &BuildDiff) {
    println!("Build {} → Build {}", diff.before, diff.after);
    render_duration_change("Total", &diff.total_change);
    println!();

    if diff.crate_changes.is_empty() {
        println!("  No crate-level changes.");
    } else {
        for change in &diff.crate_changes {
            match change {
                CrateChange::Added { crate_id, duration } => {
                    println!("  + {} (new) {:.1}s", crate_id, duration.as_secs_f64());
                }
                CrateChange::Removed { crate_id, duration } => {
                    println!("  - {} (removed) {:.1}s", crate_id, duration.as_secs_f64());
                }
                CrateChange::Changed { crate_id, change } => {
                    println!(
                        "  ~ {} {:.1}s → {:.1}s ({:+.1}s, {:+.1}%)",
                        crate_id,
                        change.before.as_secs_f64(),
                        change.after.as_secs_f64(),
                        change.abs_delta_ms as f64 / 1000.0,
                        change.pct_delta,
                    );
                }
                CrateChange::Unchanged { crate_id, duration } => {
                    println!("  = {} {:.1}s", crate_id, duration.as_secs_f64());
                }
            }
        }
    }

    println!();
    println!(
        "Critical path (before): {}",
        diff.critical_path_before.join(" → ")
    );
    println!(
        "Critical path (after):  {}",
        diff.critical_path_after.join(" → ")
    );
}

fn render_duration_change(label: &str, change: &DurationChange) {
    println!(
        "  {}: {:.1}s → {:.1}s ({:+.1}s, {:+.1}%)",
        label,
        change.before.as_secs_f64(),
        change.after.as_secs_f64(),
        change.abs_delta_ms as f64 / 1000.0,
        change.pct_delta,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Build, BuildDiff, BuildId, CrateChange, CrateId, DurationChange};
    use std::time::Duration;

    // ---- Cli::parse tests (pass with current implementation) -------------

    #[test]
    fn parse_ls_defaults_last_to_10() {
        let cli = Cli::try_parse_from(["cargo-chrono", "ls"]).unwrap();
        match cli.command {
            Command::Ls { last } => assert_eq!(last, 10),
            other => panic!("expected Ls, got {:?}", other),
        }
    }

    #[test]
    fn parse_ls_with_explicit_last() {
        let cli = Cli::try_parse_from(["cargo-chrono", "ls", "--last", "5"]).unwrap();
        match cli.command {
            Command::Ls { last } => assert_eq!(last, 5),
            _ => panic!("expected Ls"),
        }
    }

    #[test]
    fn parse_record_forwards_cargo_args() {
        let cli =
            Cli::try_parse_from(["cargo-chrono", "record", "--release", "-p", "demo"]).unwrap();
        match cli.command {
            Command::Record { cargo_args } => {
                assert_eq!(cargo_args, vec!["--release", "-p", "demo"]);
            }
            _ => panic!("expected Record"),
        }
    }

    #[test]
    fn parse_watch_forwards_cargo_args() {
        let cli = Cli::try_parse_from(["cargo-chrono", "watch", "--release"]).unwrap();
        match cli.command {
            Command::Watch { cargo_args } => {
                assert_eq!(cargo_args, vec!["--release"]);
            }
            _ => panic!("expected Watch"),
        }
    }

    #[test]
    fn parse_diff_requires_two_ids() {
        let cli = Cli::try_parse_from(["cargo-chrono", "diff", "3", "7"]).unwrap();
        match cli.command {
            Command::Diff { before, after } => {
                assert_eq!(before, 3);
                assert_eq!(after, 7);
            }
            _ => panic!("expected Diff"),
        }
    }

    #[test]
    fn parse_diff_missing_args_errors() {
        let result = Cli::try_parse_from(["cargo-chrono", "diff", "1"]);
        assert!(result.is_err());
    }

    // ---- render smoke tests (pass with current implementation) -----------

    #[test]
    fn render_ls_empty_does_not_panic() {
        render_ls(&[]);
    }

    #[test]
    fn render_ls_with_rows_does_not_panic() {
        let builds = vec![Build {
            id: BuildId(1),
            started_at: "2025-04-19T12:00:00Z".to_string(),
            finished_at: Some("2025-04-19T12:00:05Z".to_string()),
            commit_hash: Some("abc123".to_string()),
            cargo_args: "[\"build\"]".to_string(),
            profile: "dev".to_string(),
            success: Some(true),
            total_duration: Some(Duration::from_millis(5000)),
        }];
        render_ls(&builds);
    }

    #[test]
    fn render_diff_does_not_panic() {
        let diff = BuildDiff {
            before: BuildId(1),
            after: BuildId(2),
            total_change: DurationChange {
                before: Duration::from_secs(10),
                after: Duration::from_secs(12),
                abs_delta_ms: 2000,
                pct_delta: 20.0,
            },
            crate_changes: vec![
                CrateChange::Added {
                    crate_id: CrateId {
                        name: "new-crate".into(),
                        version: None,
                    },
                    duration: Duration::from_millis(500),
                },
                CrateChange::Changed {
                    crate_id: CrateId {
                        name: "foo".into(),
                        version: None,
                    },
                    change: DurationChange {
                        before: Duration::from_secs(1),
                        after: Duration::from_secs(2),
                        abs_delta_ms: 1000,
                        pct_delta: 100.0,
                    },
                },
            ],
            critical_path_before: vec!["a".into(), "b".into()],
            critical_path_after: vec!["a".into(), "b".into(), "c".into()],
        };
        render_diff(&diff);
    }

    #[test]
    fn render_diff_empty_crate_changes_does_not_panic() {
        let diff = BuildDiff {
            before: BuildId(1),
            after: BuildId(2),
            total_change: DurationChange {
                before: Duration::from_secs(1),
                after: Duration::from_secs(1),
                abs_delta_ms: 0,
                pct_delta: 0.0,
            },
            crate_changes: vec![],
            critical_path_before: vec![],
            critical_path_after: vec![],
        };
        render_diff(&diff);
    }
}
