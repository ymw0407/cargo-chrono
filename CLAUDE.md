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

## Module ownership (hard rule)

| Role | Owned modules |
|------|---------------|
| **Integrator** | `src/model/`, `src/cli/`, `src/supervisor/`, `src/parser/`, `src/main.rs`, `Cargo.toml` |
| **Data** | `src/persist/`, `src/diff/` |
| **Realtime** | `src/broker/`, `src/anomaly/`, `src/tui/` |

**Do not modify code in modules you do not own.** If your change requires a
cross-role edit, file an issue and tag the relevant owner — see
[`docs/internal/ROLE_OWNERSHIP.md`](docs/internal/ROLE_OWNERSHIP.md).

## Dependency direction (hard rule)

```
model/  ← every module may import; model/ must not depend on any other src/ module
Data ↔ Realtime          not allowed in either direction
Realtime → Data          only via the persist::BuildRepository trait
                         (no SqliteRepository imports)
main.rs                  the single assembly point for cross-role wiring
```

Code that violates these rules will be rejected. `use crate::tui` inside
`persist/` is forbidden.

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
- `feat/<role>/<topic>`, `fix/<role>/<topic>`, `test/<role>/<topic>`
- Examples: `feat/data/sqlite-crud`, `fix/realtime/tui-crash-on-exit`

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
- Only the Integrator role modifies `Cargo.toml`. If you need a new
  dependency, ask the Integrator.

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
