# CLAUDE.md — cargo-chronoscope

Rules and context that AI coding assistants (Claude Code, etc.) must follow when
working in this repository.

## Project overview

`cargo-chronoscope` is a CLI tool that observes Cargo's build event stream and
records, diffs, and visualises Rust build performance. It exposes four
commands (`record`, `watch`, `ls`, `diff`) and was originally built by a
three-person team that split ownership by module (see
[`docs/internal/ROLE_OWNERSHIP.md`](docs/internal/ROLE_OWNERSHIP.md)).

## Build & verification commands

```bash
cargo check                          # type-check
cargo test                           # run tests
cargo clippy -- -D warnings          # lint (treat warnings as errors)
cargo fmt --check                    # format check
cargo run --example ratatui_hello    # TUI demo
```

Every PR **must** pass `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
before being submitted.

## Architecture rules (hard)

These rules apply regardless of authorship. Code that violates them will be
rejected.

```
model/                ← every module may import; model/ must not depend on any other src/ module
persist/ ↔ tui/       not allowed in either direction
(also persist/ ↔ broker/ and persist/ ↔ anomaly/, both directions)
tui/ → persist/       only via the persist::BuildRepository trait
                      (no SqliteRepository imports)
main.rs               the single assembly point for cross-side wiring
```

`use crate::tui` inside `persist/` is forbidden, and vice versa.

## Review routing

PR reviews are routed automatically by [`.github/CODEOWNERS`](.github/CODEOWNERS) —
do not gate changes on a "module owner" check yourself. The original
three-person role split (Integrator / Data / Realtime) is preserved in
[`docs/internal/ROLE_OWNERSHIP.md`](docs/internal/ROLE_OWNERSHIP.md) as
historical context, not as an active gating rule. External contributions to
any module are welcome.

## Code conventions

### Error handling
- Public APIs return `anyhow::Result<T>`.
- Module-internal error types use `thiserror`.
- `unwrap()` / `expect()` are allowed only in test code. Production code uses
  the `?` operator.

### Async
- All async runs on the tokio runtime.
- For traits that need `async fn`, use `#[async_trait]`.
- Channels are bounded `tokio::sync::mpsc`, default capacity 1024.
- Cancellation is propagated via `tokio_util::sync::CancellationToken`.

### Style
- Follow `cargo fmt` (default rustfmt config).
- Every public function, struct, enum, and trait needs a `///` doc comment.
- Doc comments specify the contract via `# Arguments`, `# Returns`, `# Errors`
  sections.
- Every module file must start with a `//!` module-level doc comment.

### Commits
- [Conventional Commits](https://www.conventionalcommits.org/): `<type>(<scope>): <description>`
- type: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`, `style`
- scope: module name (`model`, `cli`, `supervisor`, `parser`, `persist`,
  `diff`, `broker`, `anomaly`, `tui`, `main`)
- Example: `feat(persist): implement begin_build with SQLite INSERT`
- Full rules: [`.github/COMMIT_CONVENTION.md`](.github/COMMIT_CONVENTION.md).

### Branches
- `<type>/<topic>` — for example `feat/sqlite-crud`, `fix/tui-crash-on-exit`.
- Legacy `<type>/<role>/<topic>` form (`feat/data/sqlite-crud`) is still
  accepted for collaborators on the original team.

## Core architecture patterns

### Event pipeline (record mode)
```
Supervisor → mpsc<String> → Parser → mpsc<BuildEvent> → Persister → DB
```

### Event pipeline (watch mode)
```
Supervisor → Parser → Broker ─┬→ Persister → DB
                              └→ TUI → Terminal
```

### BuildEvent stream contract
- First event: always `BuildStarted`.
- Last event: always `BuildFinished`.
- `CompilationFinished` always carries `duration`, `started_at`, `finished_at`.

### DB location
`<project_root>/.cargo-chronoscope/history.db` (SQLite, WAL mode).

### BuildId allocation
Issued by the Persister on `BuildStarted` via SQLite `AUTOINCREMENT`.

## Testing rules

- Every public function has a unit test.
- Tests live in the same file under `#[cfg(test)] mod tests {}`.
- Cross-module integration tests go in `tests/`.
- Test fixtures go in `tests/fixtures/`.
- DB tests use `tempfile::TempDir` for an isolated database.
- Async tests use `#[tokio::test]`.

## Notes

- Do **not** use the `cargo_metadata` crate. Parse cargo's JSON directly with
  `serde_json`.
- `rusqlite::Connection` is **not** `Sync`. Wrap it in `tokio::sync::Mutex`.
- The TUI uses raw mode. Terminal restoration must be guaranteed even on
  panic (RAII guard + panic hook).
- `Cargo.toml` changes (including new dependencies) require maintainer
  review.

## Reference documents

- [`README.md`](README.md) — user-facing English documentation.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — how to contribute (external contributors).
- [`.github/COMMIT_CONVENTION.md`](.github/COMMIT_CONVENTION.md) — detailed commit rules.
- [`.github/CONTRIBUTING.md`](.github/CONTRIBUTING.md) — detailed dev workflow.
- [`docs/internal/`](docs/internal/) — historical planning docs (in Korean):
  - `DESIGN.md` — full design (scenarios, architecture, schema, role split).
  - `CONCURRENCY.md` — anticipated race conditions and mitigations.
  - `ONBOARDING.md` — Day-1 checklist per role.
  - `AGENTS.md` — per-role AI assistant collaboration guide.
  - `ROLE_OWNERSHIP.md` — module ownership / GitHub ID mapping (English).
  - `PROJECT_HISTORY.md` — completion record (English).
