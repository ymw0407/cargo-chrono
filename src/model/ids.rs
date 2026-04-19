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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_id_display_uses_hash_prefix() {
        assert_eq!(BuildId(42).to_string(), "#42");
        assert_eq!(BuildId(1).to_string(), "#1");
        assert_eq!(BuildId(0).to_string(), "#0");
    }

    #[test]
    fn build_id_is_copy() {
        let a = BuildId(7);
        let b = a; // Copy, not move
        assert_eq!(a, b);
    }

    #[test]
    fn crate_id_display_with_version() {
        let id = CrateId {
            name: "serde".to_string(),
            version: Some("1.0.210".to_string()),
        };
        assert_eq!(id.to_string(), "serde v1.0.210");
    }

    #[test]
    fn crate_id_display_without_version() {
        let id = CrateId {
            name: "my-workspace-crate".to_string(),
            version: None,
        };
        assert_eq!(id.to_string(), "my-workspace-crate");
    }

    #[test]
    fn crate_id_equality_considers_both_fields() {
        let a = CrateId {
            name: "foo".to_string(),
            version: Some("1.0".to_string()),
        };
        let b = CrateId {
            name: "foo".to_string(),
            version: Some("1.0".to_string()),
        };
        let c = CrateId {
            name: "foo".to_string(),
            version: Some("2.0".to_string()),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
