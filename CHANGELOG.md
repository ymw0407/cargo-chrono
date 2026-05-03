# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.5] - 2026-05-03

### Changed
- Repository renamed from `ymw0407/cargo-chrono` to `ymw0407/cargo-chronoscope` to match the published crate name. All in-repo links updated to the canonical URL; old URLs continue to redirect.
- `cd cargo-chrono` clone instructions updated to `cd cargo-chronoscope` to match the new default directory name produced by `git clone`.

## [0.1.4] - 2026-05-03

### Fixed
- `repository` and `homepage` fields in `Cargo.toml` corrected to point at the actual repository (`cargo-chrono`); the previous values returned 404 from crates.io.
- README and the example workflow no longer reference the non-existent `cargo-chronoscope` repository slug.
- `LICENSE` normalized to canonical MIT so GitHub correctly detects the license.

### Added
- `SECURITY.md` with private vulnerability reporting instructions.
- `CODE_OF_CONDUCT.md` adopting Contributor Covenant 2.1.
- `CHANGELOG.md` (this file).
- `.github/dependabot.yml` for weekly cargo and github-actions updates.

### Changed
- README "Status" section updated to reflect that Linux x86_64 and macOS x86_64/aarch64 are now exercised by the release workflow.

## [0.1.3] - 2026-05-03

### Added
- `Cargo.lock` is now committed to version control for reproducible builds and deterministic CI cache hits.
- `cargo binstall` is invoked with `--locked` in the composite GitHub Action.

### Fixed
- Release workflow now creates the GitHub Release object before the binary upload jobs run, so `taiki-e/upload-rust-binary-action` no longer retries indefinitely.

## [0.1.2] - 2026-05-03

### Added
- Release workflow ships pre-built binaries for `x86_64-unknown-linux-gnu`, `x86_64-apple-darwin`, and `aarch64-apple-darwin`. Consumable via `cargo binstall cargo-chronoscope`.
- `[package.metadata.binstall]` template in `Cargo.toml` so binstall can resolve the archive URL.

### Fixed
- GitHub Action description trimmed to fit the Marketplace 125-character limit.

## [0.1.1] - 2026-05-03

### Added
- Composite GitHub Action wrapping the CLI into a one-step CI flow with sticky PR diff comments.
- `--format json` output mode used by the action's diff renderer.

## [0.1.0] - 2026-05-03

- Initial public release on crates.io.
- Commands: `record`, `watch`, `ls`, `diff`.
- SQLite-backed history at `<workspace>/.cargo-chronoscope/history.db`.
- Anomaly classifier (mean ± 2σ) and ratatui dashboard.

[Unreleased]: https://github.com/ymw0407/cargo-chronoscope/compare/v0.1.5...HEAD
[0.1.5]: https://github.com/ymw0407/cargo-chronoscope/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/ymw0407/cargo-chronoscope/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/ymw0407/cargo-chronoscope/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/ymw0407/cargo-chronoscope/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ymw0407/cargo-chronoscope/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ymw0407/cargo-chronoscope/releases/tag/v0.1.0
