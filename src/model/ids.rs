//! Core identifier types used throughout cargo-chrono.

use std::fmt;

/// Unique identifier for a recorded build, issued by the database on INSERT.
///
/// Wraps a SQLite `INTEGER PRIMARY KEY AUTOINCREMENT` value.
/// Displayed as `#42` for human-readable output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BuildId(pub i64);

impl fmt::Display for BuildId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Identifies a crate being compiled.
///
/// `version` is `None` when the crate is a path dependency or workspace member
/// (Cargo does not always emit a version for those).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrateId {
    pub name: String,
    pub version: Option<String>,
}

impl fmt::Display for CrateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.version {
            Some(v) => write!(f, "{} v{}", self.name, v),
            None => write!(f, "{}", self.name),
        }
    }
}
