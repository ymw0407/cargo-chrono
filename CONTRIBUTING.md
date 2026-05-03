# Contributing to cargo-chronoscope

Thanks for your interest in `cargo-chronoscope`. This document explains how to
report issues, propose changes, and get a pull request merged.

## Quick links

- **Bugs / feature requests**: [open an issue](https://github.com/ymw0407/cargo-chronoscope/issues/new/choose)
- **Code style & commit format**: [`.github/COMMIT_CONVENTION.md`](.github/COMMIT_CONVENTION.md)
- **Detailed dev workflow**: [`.github/CONTRIBUTING.md`](.github/CONTRIBUTING.md)
- **Module ownership**: [`docs/internal/ROLE_OWNERSHIP.md`](docs/internal/ROLE_OWNERSHIP.md)

## Reporting issues

Use the issue templates at
[`.github/ISSUE_TEMPLATE/`](.github/ISSUE_TEMPLATE/) — they exist for bugs,
feature requests, and tasks. A good bug report includes:

- The exact command you ran.
- The expected behaviour and the actual behaviour.
- Your OS, `rustc --version`, and the `cargo-chronoscope` version (or commit
  hash).
- A minimal reproduction, ideally a small Rust project we can run against.

## Proposing changes

For anything beyond a typo fix, **open an issue first** so we can agree on the
approach before you spend time on a PR. Drive-by refactors that touch many
modules are unlikely to be accepted without prior discussion.

The codebase has a strict module-ownership rule (Integrator / Data / Realtime).
If your change spans modules owned by different roles, mention this in the
issue so the right people can review.

## Setting up your environment

```bash
git clone https://github.com/ymw0407/cargo-chronoscope.git
cd cargo-chronoscope

# Pre-flight check — must pass before opening a PR.
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Tested on macOS aarch64 + stable Rust. Linux and Windows should work but are
not yet covered by CI — if you hit platform-specific issues, please file them.

## Pull request workflow

1. Fork the repo (or create a branch if you're a collaborator).
2. Branch name: `<type>/<role>/<topic>` — for example
   `feat/data/sqlite-crud` or `fix/realtime/tui-crash`. See
   [`.github/COMMIT_CONVENTION.md`](.github/COMMIT_CONVENTION.md) for the
   full naming scheme.
3. Make your change. Keep the diff focused on one logical thing.
4. Add tests for new behaviour. We do not merge new public APIs without unit
   tests.
5. Update relevant documentation (`README.md` for user-facing changes,
   inline doc comments for API changes).
6. Ensure `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
   all pass locally.
7. Open the PR using the template
   ([`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md)).
   Link the related issue (`Closes #N`).
8. Address review comments by pushing new commits — do not force-push during
   review unless asked.

## Commit messages

Conventional Commits format:

```
<type>(<scope>): <description>

[optional body]

[optional footer — e.g. Closes #123]
```

`<scope>` is the module name (`model`, `cli`, `supervisor`, `parser`,
`persist`, `diff`, `broker`, `anomaly`, `tui`, `main`).

Multi-module commits should be split. For the full rule set and examples see
[`.github/COMMIT_CONVENTION.md`](.github/COMMIT_CONVENTION.md).

## Code style

- Follow `cargo fmt` (default rustfmt config).
- Production code uses `?` for error propagation. `unwrap()` / `expect()` are
  allowed only in tests.
- Every public function, struct, enum, and trait carries a `///` doc comment
  with `# Arguments`, `# Returns`, `# Errors` sections where relevant.
- Every module file starts with a `//!` module-level doc comment.

## Module ownership

The codebase splits responsibility across three roles. PRs that touch a
module owned by a different role need explicit sign-off from that owner.

| Role | Owned modules |
|------|---------------|
| Integrator | `src/model/`, `src/cli/`, `src/supervisor/`, `src/parser/`, `src/main.rs`, `Cargo.toml` |
| Data | `src/persist/`, `src/diff/` |
| Realtime | `src/broker/`, `src/anomaly/`, `src/tui/` |

Dependency rules:
- `model/` may be imported from anywhere; nothing else may be imported into
  `model/`.
- `Data` and `Realtime` modules must **not** import each other directly.
- `Realtime → Data` is allowed only via the `persist::BuildRepository`
  trait — never the concrete `SqliteRepository`.
- `main.rs` is the only place where modules from different roles are wired
  together.

## Code of conduct

Be excellent to each other. Disagreements about technical direction are
fine; personal attacks, harassment, or discriminatory language are not. The
maintainers reserve the right to remove comments, lock threads, or block
users that violate this.

## License

By contributing, you agree that your contributions will be licensed under
the [MIT License](LICENSE) as part of the project.
