//! Database schema migrations.
//!
//! Contains the SQL DDL for creating the cargo-chrono tables and indices.
//! Called by `SqliteRepository::open()` on first use.

use rusqlite::Connection;

/// SQL statements for the initial schema (version 1).
const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS builds (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at        TEXT    NOT NULL,
    finished_at       TEXT,
    commit_hash       TEXT,
    cargo_args        TEXT    NOT NULL,
    profile           TEXT    NOT NULL,
    success           INTEGER,
    total_duration_ms INTEGER
);

CREATE TABLE IF NOT EXISTS crate_compilations (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    build_id        INTEGER NOT NULL REFERENCES builds(id),
    crate_name      TEXT    NOT NULL,
    crate_version   TEXT,
    kind            TEXT    NOT NULL,
    started_at      TEXT    NOT NULL,
    finished_at     TEXT    NOT NULL,
    duration_ms     INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_compilations_build
    ON crate_compilations(build_id);

CREATE INDEX IF NOT EXISTS idx_compilations_crate
    ON crate_compilations(crate_name);

CREATE INDEX IF NOT EXISTS idx_builds_started
    ON builds(started_at DESC);
"#;

/// Run all pending migrations on the given database connection.
///
/// Currently only applies the V1 schema. Future versions will add
/// a `schema_version` table and incremental migrations.
///
/// # Errors
///
/// Returns an error if any SQL statement fails to execute.
pub(crate) fn run_migrations(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(SCHEMA_V1)?;
    Ok(())
}
