//! Database schema migrations.
//!
//! Contains the SQL DDL for creating the cargo-chronoscope tables and indices.
//! Called by `SqliteRepository::open()` on first use.
//!
//! # Concurrency
//!
//! Migrations run inside a `BEGIN IMMEDIATE` transaction so a second
//! `cargo-chronoscope` process opening the same DB file blocks until the first
//! finishes (or times out). The current `PRAGMA user_version` is checked
//! inside the transaction; if it already matches `SCHEMA_VERSION`, the
//! migration is a no-op. This keeps the open path safe against the race in
//! issue #3.

use rusqlite::{Connection, TransactionBehavior};

/// Bumped whenever the schema changes. Stored in `PRAGMA user_version`.
const SCHEMA_VERSION: i32 = 1;

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
/// Atomic: opens an `IMMEDIATE` transaction so concurrent openers serialise
/// on the writer lock, and uses `PRAGMA user_version` to skip work that has
/// already been applied. Idempotent.
///
/// # Errors
///
/// Returns an error if any SQL statement fails to execute or if the writer
/// lock cannot be acquired within the connection's `busy_timeout`.
pub(crate) fn run_migrations(conn: &mut Connection) -> anyhow::Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    let current: i32 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current >= SCHEMA_VERSION {
        // Another process already migrated this DB. Nothing to do.
        return Ok(());
    }

    tx.execute_batch(SCHEMA_V1)?;
    // PRAGMA does not accept bound parameters, so format the version in.
    tx.execute_batch(&format!("PRAGMA user_version = {}", SCHEMA_VERSION))?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_sets_user_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        let v: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn second_run_is_a_noop() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        // Second call must not error and must leave user_version unchanged.
        run_migrations(&mut conn).unwrap();
        let v: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }
}
