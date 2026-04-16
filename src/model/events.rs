//! Build event types emitted by the Parser.
//!
//! The event stream follows a strict ordering contract:
//! 1. First event is always `BuildStarted`
//! 2. Zero or more `CompilationStarted` / `CompilationFinished` / `CompilerMessage` events
//! 3. Last event is always `BuildFinished`
//!
//! `CompilationFinished` always contains the computed `duration`
//! (the Parser internally matches start/finish pairs).

use std::time::Duration;

use crate::model::ids::CrateId;

/// A build event produced by the Parser from Cargo's JSON output.
#[derive(Debug, Clone)]
pub enum BuildEvent {
    /// Emitted once at the beginning of a build.
    BuildStarted {
        /// Timestamp when the build started (ISO 8601).
        at: String,
        /// Git commit hash at the time of the build, if available.
        commit_hash: Option<String>,
        /// The cargo arguments used for this build.
        cargo_args: Vec<String>,
        /// The build profile (dev, release, custom).
        profile: BuildProfile,
    },

    /// Emitted when a crate begins compiling.
    CompilationStarted {
        crate_id: CrateId,
        kind: CrateKind,
        /// Timestamp when compilation started (ISO 8601).
        at: String,
    },

    /// Emitted when a crate finishes compiling.
    /// The Parser guarantees that `duration` is populated.
    CompilationFinished {
        crate_id: CrateId,
        kind: CrateKind,
        /// Timestamp when compilation started (ISO 8601).
        started_at: String,
        /// Timestamp when compilation finished (ISO 8601).
        finished_at: String,
        /// Wall-clock compilation duration.
        duration: Duration,
    },

    /// A diagnostic message from the compiler for a specific crate.
    CompilerMessage {
        crate_id: CrateId,
        level: MessageLevel,
        text: String,
    },

    /// Emitted once at the end of a build.
    BuildFinished {
        /// Whether the build succeeded.
        success: bool,
        /// Total wall-clock duration of the entire build.
        total_duration: Duration,
        /// Timestamp when the build finished (ISO 8601).
        at: String,
    },
}

/// The kind of crate target being compiled.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CrateKind {
    Lib,
    Bin,
    BuildScript,
    ProcMacro,
    Test,
    Example,
    Bench,
    Unknown,
}

impl std::fmt::Display for CrateKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CrateKind::Lib => "lib",
            CrateKind::Bin => "bin",
            CrateKind::BuildScript => "build-script",
            CrateKind::ProcMacro => "proc-macro",
            CrateKind::Test => "test",
            CrateKind::Example => "example",
            CrateKind::Bench => "bench",
            CrateKind::Unknown => "unknown",
        };
        write!(f, "{}", s)
    }
}

/// Build profile used by Cargo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildProfile {
    Dev,
    Release,
    Custom,
}

impl std::fmt::Display for BuildProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BuildProfile::Dev => "dev",
            BuildProfile::Release => "release",
            BuildProfile::Custom => "custom",
        };
        write!(f, "{}", s)
    }
}

/// Severity level of a compiler diagnostic message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageLevel {
    Warning,
    Error,
    Note,
}
