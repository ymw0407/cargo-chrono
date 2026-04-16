//! cargo-chrono — Cargo build performance observer.
//!
//! Entry point that parses CLI arguments and dispatches to the appropriate
//! command handler. Assembles all async tasks and manages graceful shutdown.

// Allow dead_code in the skeleton phase — all public APIs are intentionally
// defined but not yet wired up. Remove this once modules are implemented.
#![allow(dead_code)]

mod anomaly;
mod broker;
mod cli;
mod diff;
mod model;
mod parser;
mod persist;
mod supervisor;
mod tui;

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tokio_util::sync::CancellationToken;

use crate::cli::{Cli, Command};
use crate::model::BuildId;
use crate::persist::BuildRepository;

/// Default DB directory name within the project root.
const DB_DIR: &str = ".cargo-chrono";
/// Default DB file name.
const DB_FILE: &str = "history.db";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cancel = CancellationToken::new();

    // Set up Ctrl-C handler.
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for Ctrl-C");
        cancel_clone.cancel();
    });

    // Determine workspace directory and DB path.
    let workspace_dir = std::env::current_dir()?;
    let db_dir = workspace_dir.join(DB_DIR);
    std::fs::create_dir_all(&db_dir)?;
    let db_path = db_dir.join(DB_FILE);

    match cli.command {
        Command::Record { cargo_args } => {
            cmd_record(cargo_args, workspace_dir, &db_path, cancel).await?;
        }
        Command::Watch { cargo_args } => {
            cmd_watch(cargo_args, workspace_dir, &db_path, cancel).await?;
        }
        Command::Ls { last } => {
            cmd_ls(&db_path, last).await?;
        }
        Command::Diff { before, after } => {
            cmd_diff(&db_path, before, after).await?;
        }
    }

    Ok(())
}

/// Record a build: Supervisor → Parser → Persister (3-task pipeline).
async fn cmd_record(
    cargo_args: Vec<String>,
    workspace_dir: PathBuf,
    db_path: &std::path::Path,
    _cancel: CancellationToken,
) -> anyhow::Result<()> {
    let commit_hash = read_git_head();
    let profile = infer_profile(&cargo_args);

    let repo = Arc::new(persist::SqliteRepository::open(db_path).await?);

    let (line_rx, _handle) = supervisor::spawn_build(cargo_args.clone(), workspace_dir).await?;

    let config = parser::ParserConfig {
        commit_hash,
        cargo_args,
        profile,
    };
    let event_rx = parser::run_parser(line_rx, config).await?;

    let build_id = persist::run_persister(repo, event_rx).await?;
    println!("Build {} recorded.", build_id);

    Ok(())
}

/// Watch a build: Supervisor → Parser → Broker → (Persister + TUI) fan-out.
async fn cmd_watch(
    cargo_args: Vec<String>,
    workspace_dir: PathBuf,
    db_path: &std::path::Path,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let commit_hash = read_git_head();
    let profile = infer_profile(&cargo_args);

    let repo = Arc::new(persist::SqliteRepository::open(db_path).await?);

    let (line_rx, _handle) = supervisor::spawn_build(cargo_args.clone(), workspace_dir).await?;

    let config = parser::ParserConfig {
        commit_hash,
        cargo_args,
        profile,
    };
    let event_rx = parser::run_parser(line_rx, config).await?;

    // Set up broker with two subscribers: persister and TUI.
    let mut event_broker = broker::EventBroker::new();
    let persister_rx = event_broker.subscribe(1024);
    let tui_rx = event_broker.subscribe(1024);

    let repo_clone = repo.clone();
    let cancel_clone = cancel.clone();

    // Run all tasks concurrently.
    let (broker_result, persister_result, tui_result) = tokio::try_join!(
        event_broker.publish_loop(event_rx, cancel.clone()),
        persist::run_persister(repo_clone, persister_rx),
        tui::run_tui(tui_rx, repo, cancel_clone),
    )?;

    let _ = (broker_result, tui_result);
    println!("Build {} recorded.", persister_result);

    Ok(())
}

/// List recent builds.
async fn cmd_ls(db_path: &std::path::Path, last: usize) -> anyhow::Result<()> {
    let repo = persist::SqliteRepository::open(db_path).await?;
    let builds = repo.list_builds(last).await?;
    cli::render_ls(&builds);
    Ok(())
}

/// Diff two builds.
async fn cmd_diff(db_path: &std::path::Path, before: i64, after: i64) -> anyhow::Result<()> {
    let repo = persist::SqliteRepository::open(db_path).await?;
    let build_diff = diff::compute_diff(&repo, BuildId(before), BuildId(after)).await?;
    cli::render_diff(&build_diff);
    Ok(())
}

/// Attempt to read the current git HEAD commit hash.
fn read_git_head() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
}

/// Infer the build profile from cargo arguments.
fn infer_profile(cargo_args: &[String]) -> model::BuildProfile {
    if cargo_args.iter().any(|a| a == "--release") {
        model::BuildProfile::Release
    } else {
        model::BuildProfile::Dev
    }
}
