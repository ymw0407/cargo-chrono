# Contributing to cargo-chronoscope

Thanks for your interest in `cargo-chronoscope`. This document explains how to
report issues, propose changes, and get a pull request merged.

## Quick links

- **Bugs / feature requests**: [open an issue](https://github.com/ymw0407/cargo-chronoscope/issues/new/choose)
- **Code style & commit format**: [`.github/COMMIT_CONVENTION.md`](.github/COMMIT_CONVENTION.md)
- **Detailed dev workflow**: [`.github/CONTRIBUTING.md`](.github/CONTRIBUTING.md)
- **Review routing**: [`.github/CODEOWNERS`](.github/CODEOWNERS)
- **Original role split (historical)**: [`docs/internal/ROLE_OWNERSHIP.md`](docs/internal/ROLE_OWNERSHIP.md)

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

External contributions are welcome to any module. Review routing is handled
automatically by [`.github/CODEOWNERS`](.github/CODEOWNERS) — you don't need
to figure out who owns what before opening a PR.

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
2. Branch name: `<type>/<topic>` — for example `fix/tui-crash-on-exit`.
   Collaborators on the original team may also use the
   `<type>/<role>/<topic>` form (`feat/data/sqlite-crud`); both are accepted.
   See [`.github/COMMIT_CONVENTION.md`](.github/COMMIT_CONVENTION.md) for the
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

## Architecture rules

These rules are enforced regardless of who authors the change — they keep the
codebase from drifting into circular or leaky dependencies.

- `model/` may be imported from anywhere; nothing else may be imported into
  `model/`.
- `src/persist/` / `src/diff/` and `src/broker/` / `src/anomaly/` / `src/tui/`
  must **not** import each other directly.
- The TUI / broker / anomaly side may consume persistence only via the
  `persist::BuildRepository` trait — never the concrete `SqliteRepository`.
- `main.rs` is the only place where modules from different sides are wired
  together.

The original three-role split (Integrator / Data / Realtime) that motivated
these rules is preserved in
[`docs/internal/ROLE_OWNERSHIP.md`](docs/internal/ROLE_OWNERSHIP.md) as
historical context. Active review routing is in
[`.github/CODEOWNERS`](.github/CODEOWNERS).

## Code of conduct

Be excellent to each other. Disagreements about technical direction are
fine; personal attacks, harassment, or discriminatory language are not. The
maintainers reserve the right to remove comments, lock threads, or block
users that violate this.

## License

By contributing, you agree that your contributions will be licensed under
the [MIT License](LICENSE) as part of the project.
