//! Stable JSON wire format for CLI output.
//!
//! Domain types in `model::` deliberately do not derive `Serialize`: their
//! shape is an internal contract and may change as the persistence layer
//! evolves. The DTOs in this module define the *external* JSON schema that
//! CI integrations and downstream tooling consume, so we control its
//! stability separately.
//!
//! ## Conventions
//! - Durations are emitted as integer milliseconds.
//! - Build IDs are integers (no `#` prefix), so they can be passed back to
//!   `cargo-chronoscope diff` directly.
//! - `crate_id` is a flat object `{ "name": "...", "version": "..." | null }`.

use serde::Serialize;

use crate::model::{Build, BuildDiff, CrateChange, CrateId, DurationChange};

/// JSON envelope for `cargo-chronoscope ls --format json`.
#[derive(Debug, Serialize)]
pub struct LsJson {
    pub builds: Vec<BuildJson>,
}

/// JSON shape of a single recorded build.
#[derive(Debug, Serialize)]
pub struct BuildJson {
    pub id: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub commit_hash: Option<String>,
    pub cargo_args: String,
    pub profile: String,
    pub success: Option<bool>,
    pub duration_ms: Option<u128>,
}

impl From<&Build> for BuildJson {
    fn from(b: &Build) -> Self {
        Self {
            id: b.id.0,
            started_at: b.started_at.clone(),
            finished_at: b.finished_at.clone(),
            commit_hash: b.commit_hash.clone(),
            cargo_args: b.cargo_args.clone(),
            profile: b.profile.clone(),
            success: b.success,
            duration_ms: b.total_duration.map(|d| d.as_millis()),
        }
    }
}

/// JSON shape returned by `cargo-chronoscope diff --format json`.
#[derive(Debug, Serialize)]
pub struct DiffJson {
    pub before: i64,
    pub after: i64,
    pub total: DurationChangeJson,
    pub crate_changes: Vec<CrateChangeJson>,
    pub critical_path_before: Vec<String>,
    pub critical_path_after: Vec<String>,
}

/// JSON shape of a duration change between two builds.
#[derive(Debug, Serialize)]
pub struct DurationChangeJson {
    pub before_ms: u128,
    pub after_ms: u128,
    pub delta_ms: i64,
    pub pct_delta: f64,
}

impl From<&DurationChange> for DurationChangeJson {
    fn from(c: &DurationChange) -> Self {
        Self {
            before_ms: c.before.as_millis(),
            after_ms: c.after.as_millis(),
            delta_ms: c.abs_delta_ms,
            pct_delta: c.pct_delta,
        }
    }
}

/// JSON shape of a crate identifier.
#[derive(Debug, Serialize)]
pub struct CrateIdJson {
    pub name: String,
    pub version: Option<String>,
}

impl From<&CrateId> for CrateIdJson {
    fn from(c: &CrateId) -> Self {
        Self {
            name: c.name.clone(),
            version: c.version.clone(),
        }
    }
}

/// JSON shape of a single crate change. Tagged by `kind` for easy parsing.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CrateChangeJson {
    Added {
        crate_id: CrateIdJson,
        duration_ms: u128,
    },
    Removed {
        crate_id: CrateIdJson,
        duration_ms: u128,
    },
    Changed {
        crate_id: CrateIdJson,
        change: DurationChangeJson,
    },
    Unchanged {
        crate_id: CrateIdJson,
        duration_ms: u128,
    },
}

impl From<&CrateChange> for CrateChangeJson {
    fn from(c: &CrateChange) -> Self {
        match c {
            CrateChange::Added { crate_id, duration } => Self::Added {
                crate_id: crate_id.into(),
                duration_ms: duration.as_millis(),
            },
            CrateChange::Removed { crate_id, duration } => Self::Removed {
                crate_id: crate_id.into(),
                duration_ms: duration.as_millis(),
            },
            CrateChange::Changed { crate_id, change } => Self::Changed {
                crate_id: crate_id.into(),
                change: change.into(),
            },
            CrateChange::Unchanged { crate_id, duration } => Self::Unchanged {
                crate_id: crate_id.into(),
                duration_ms: duration.as_millis(),
            },
        }
    }
}

impl From<&BuildDiff> for DiffJson {
    fn from(d: &BuildDiff) -> Self {
        Self {
            before: d.before.0,
            after: d.after.0,
            total: (&d.total_change).into(),
            crate_changes: d.crate_changes.iter().map(Into::into).collect(),
            critical_path_before: d.critical_path_before.clone(),
            critical_path_after: d.critical_path_after.clone(),
        }
    }
}

/// Render builds as a single-line JSON envelope to stdout.
pub fn render_ls_json(builds: &[Build]) -> anyhow::Result<()> {
    let envelope = LsJson {
        builds: builds.iter().map(Into::into).collect(),
    };
    serde_json::to_writer(std::io::stdout(), &envelope)?;
    println!();
    Ok(())
}

/// Render a build diff as a single-line JSON object to stdout.
pub fn render_diff_json(diff: &BuildDiff) -> anyhow::Result<()> {
    let dto: DiffJson = diff.into();
    serde_json::to_writer(std::io::stdout(), &dto)?;
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Build, BuildDiff, BuildId, CrateChange, CrateId, DurationChange};
    use std::time::Duration;

    fn sample_build() -> Build {
        Build {
            id: BuildId(7),
            started_at: "2026-05-03T01:00:00".into(),
            finished_at: Some("2026-05-03T01:00:42".into()),
            commit_hash: Some("abc1234".into()),
            cargo_args: "[\"--release\"]".into(),
            profile: "release".into(),
            success: Some(true),
            total_duration: Some(Duration::from_millis(42_500)),
        }
    }

    #[test]
    fn build_serializes_with_flat_id_and_ms_duration() {
        let json = serde_json::to_value(BuildJson::from(&sample_build())).unwrap();
        assert_eq!(json["id"], 7);
        assert_eq!(json["duration_ms"], 42_500);
        assert_eq!(json["profile"], "release");
        assert_eq!(json["success"], true);
    }

    #[test]
    fn ls_envelope_wraps_builds_array() {
        let envelope = LsJson {
            builds: vec![BuildJson::from(&sample_build())],
        };
        let json = serde_json::to_value(&envelope).unwrap();
        assert!(json["builds"].is_array());
        assert_eq!(json["builds"][0]["id"], 7);
    }

    #[test]
    fn crate_change_is_tagged_by_kind() {
        let added: CrateChangeJson = (&CrateChange::Added {
            crate_id: CrateId {
                name: "serde".into(),
                version: Some("1.0".into()),
            },
            duration: Duration::from_millis(1_500),
        })
            .into();
        let json = serde_json::to_value(&added).unwrap();
        assert_eq!(json["kind"], "added");
        assert_eq!(json["crate_id"]["name"], "serde");
        assert_eq!(json["crate_id"]["version"], "1.0");
        assert_eq!(json["duration_ms"], 1_500);
    }

    #[test]
    fn diff_serializes_with_numeric_ids() {
        let diff = BuildDiff {
            before: BuildId(1),
            after: BuildId(2),
            total_change: DurationChange {
                before: Duration::from_millis(1_000),
                after: Duration::from_millis(1_500),
                abs_delta_ms: 500,
                pct_delta: 50.0,
            },
            crate_changes: vec![],
            critical_path_before: vec!["a".into()],
            critical_path_after: vec!["a".into(), "b".into()],
        };
        let json = serde_json::to_value(DiffJson::from(&diff)).unwrap();
        assert_eq!(json["before"], 1);
        assert_eq!(json["after"], 2);
        assert_eq!(json["total"]["delta_ms"], 500);
        assert_eq!(json["critical_path_after"].as_array().unwrap().len(), 2);
    }
}
