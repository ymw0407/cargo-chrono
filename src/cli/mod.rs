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
    println!(
        "Build {} → Build {}",
        diff.before, diff.after
    );
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
