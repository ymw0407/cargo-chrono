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
<type>/<role>/<topic>
```

- `<type>` ∈ `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`
- `<role>` ∈ `integrator`, `data`, `realtime` — the role of the PR author,
  not necessarily of every file touched.
- `<topic>` — short kebab-case summary.

Pure documentation changes that don't fit a single role may use
`docs/<topic>` (for example `docs/opensource-release-prep`).

Examples:
- `feat/data/sqlite-crud`
- `feat/realtime/broker-publish-loop`
- `fix/integrator/supervisor-kill-signal`
- `docs/api-reference`

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

## Module ownership

| Role | Owned modules | Notes |
|------|---------------|-------|
| **Integrator** | `src/model/`, `src/cli/`, `src/supervisor/`, `src/parser/`, `src/main.rs`, `Cargo.toml` | Owns the wire format and the assembly point. `Cargo.toml` changes (including new dependencies) must go through the Integrator. |
| **Data** | `src/persist/`, `src/diff/` | Owns persistence and analytical comparison. The `BuildRepository` trait is the only Data API that other roles may consume. |
| **Realtime** | `src/broker/`, `src/anomaly/`, `src/tui/` | Owns event distribution, anomaly classification, and the TUI. May read from `BuildRepository` but not import any concrete persistence type. |

For the authoritative GitHub-ID-to-role mapping see
[`docs/internal/ROLE_OWNERSHIP.md`](../docs/internal/ROLE_OWNERSHIP.md).

## Cross-role coordination

When a fix or feature unavoidably touches another role's module:

1. File a GitHub issue describing the change. Tag the affected role owner.
2. Either (a) ask that owner to take the change, or (b) get explicit sign-off
   in the issue before opening the PR.
3. The PR title or body must call out the cross-role touch
   (`(touches persist/)`).

Recent precedents:
- [PR #10](https://github.com/ymw0407/cargo-chronoscope/pull/10) added
  `BuildRepository::delete_build` (Data) so the Integrator could discard
  cancelled builds. Coordinated via [issue #9](https://github.com/ymw0407/cargo-chronoscope/issues/9).
- [PR #11](https://github.com/ymw0407/cargo-chronoscope/pull/11) was pure Data work
  (busy_timeout, atomic migrations). Tracked via
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
- Does the change respect module ownership and dependency direction?
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
