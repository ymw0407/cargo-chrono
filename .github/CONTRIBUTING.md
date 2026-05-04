# Contributing Guide (detailed)

Thanks for contributing to `cargo-chronoscope`. For a high-level overview see
the root [`CONTRIBUTING.md`](../CONTRIBUTING.md). This document covers the
detailed development workflow used by the team and regular contributors.

## Development environment

```bash
# Clone
git clone https://github.com/ymw0407/cargo-chronoscope.git
cd cargo-chronoscope

# Type-check
cargo check

# Run all tests
cargo test

# Lint (treat warnings as errors)
cargo clippy --all-targets -- -D warnings

# Format check
cargo fmt --check
```

## Branch strategy

```
<type>/<topic>
```

- `<type>` ∈ `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`
- `<topic>` — short kebab-case summary.

Collaborators on the original three-person team may also use the legacy
`<type>/<role>/<topic>` form (`feat/data/sqlite-crud`, `fix/realtime/...`).
Both forms are accepted.

Examples:
- `feat/sqlite-crud`
- `fix/supervisor-kill-signal`
- `docs/api-reference`
- `feat/data/sqlite-crud` (legacy form, still accepted)

## PR checklist

Before opening a PR:

- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy --all-targets -- -D warnings` passes.
- [ ] `cargo test` passes (all targets).
- [ ] New public APIs have unit tests.
- [ ] User-facing changes are reflected in `README.md`.
- [ ] Internal architecture changes are reflected in `docs/internal/` (if
  applicable).
- [ ] Commit messages follow the
  [Conventional Commits](https://www.conventionalcommits.org/) format —
  see [`COMMIT_CONVENTION.md`](COMMIT_CONVENTION.md).
- [ ] PR title summarises the change in one line; PR body uses the
  [PR template](PULL_REQUEST_TEMPLATE.md).
- [ ] Related issue is linked (`Closes #N`).

## Architecture rules

These rules are enforced regardless of authorship — they prevent the codebase
from drifting into circular or leaky dependencies.

| Module set | Role |
|---|---|
| `src/model/`, `src/cli/`, `src/supervisor/`, `src/parser/`, `src/main.rs`, `Cargo.toml` | wire format, CLI, child-process plumbing, manifest |
| `src/persist/`, `src/diff/` | persistence (SQLite) and analytical comparison; exposes the `BuildRepository` trait |
| `src/broker/`, `src/anomaly/`, `src/tui/` | event distribution, anomaly classification, TUI |

Hard rules:

- `model/` may be imported from anywhere; nothing else may be imported into
  `model/`.
- `persist/` / `diff/` and `broker/` / `anomaly/` / `tui/` must **not** import
  each other directly.
- The TUI / broker / anomaly side may consume persistence only via the
  `persist::BuildRepository` trait — never the concrete `SqliteRepository`.
- `main.rs` is the only place where modules from different sides are wired
  together.
- `Cargo.toml` changes (including new dependencies) require maintainer
  review.

External contributions are welcome to any module. Review routing is handled
automatically by [`.github/CODEOWNERS`](CODEOWNERS); the original
three-person role split is preserved in
[`docs/internal/ROLE_OWNERSHIP.md`](../docs/internal/ROLE_OWNERSHIP.md) as
historical context.

## Cross-module coordination

When a fix or feature touches multiple module sets:

1. File a GitHub issue describing the change.
2. The PR title or body should call out the cross-module touch
   (`(touches persist/)`) so reviewers notice.

Recent precedents:
- [PR #10](https://github.com/ymw0407/cargo-chronoscope/pull/10) added
  `BuildRepository::delete_build` so the run loop could discard
  cancelled builds. Coordinated via [issue #9](https://github.com/ymw0407/cargo-chronoscope/issues/9).
- [PR #11](https://github.com/ymw0407/cargo-chronoscope/pull/11) was pure
  persistence work (busy_timeout, atomic migrations). Tracked via
  [issue #3](https://github.com/ymw0407/cargo-chronoscope/issues/3).

## Testing rules

- Every public function gets a unit test in the same file under
  `#[cfg(test)] mod tests {}`.
- Cross-module integration tests live in `tests/` at the workspace root.
- Test fixtures live in `tests/fixtures/`.
- DB tests use `tempfile::TempDir` for an isolated database — never touch
  the user's `.cargo-chronoscope/`.
- Async tests use `#[tokio::test]`.

## Code style

- Run `cargo fmt` before every commit. CI rejects unformatted code.
- Production code returns `anyhow::Result<T>` from public APIs.
- Module-internal errors use `thiserror`.
- `unwrap()` / `expect()` are allowed only in test code. Production code
  uses `?`.
- Every public item carries a `///` doc comment with `# Arguments`,
  `# Returns`, and `# Errors` sections where relevant.
- Every module file starts with a `//!` module-level comment.

## Reviewer expectations

A good review checks:

- Does the change match the issue scope? Drive-by edits should be split out.
- Are there tests for the new behaviour?
- Does the change respect the dependency direction rules above?
- Is documentation updated?
- For user-facing output changes, does the PR include a sample of the new
  output?

## Reference documents

- Top-level [`README.md`](../README.md) — user-facing English documentation.
- Top-level [`CONTRIBUTING.md`](../CONTRIBUTING.md) — short overview for
  external contributors.
- [`COMMIT_CONVENTION.md`](COMMIT_CONVENTION.md) — commit message format.
- [`PULL_REQUEST_TEMPLATE.md`](PULL_REQUEST_TEMPLATE.md) — PR template.
- [`ISSUE_TEMPLATE/`](ISSUE_TEMPLATE/) — issue templates.
- [`docs/internal/`](../docs/internal/) — historical planning docs (in Korean).
