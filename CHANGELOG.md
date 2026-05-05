# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.9] - 2026-05-05

### Changed
- Release tagging consolidated: `vX.Y.Z` is now the single canonical tag for both the crate and the GitHub Action. Each release commit ships Cargo.toml + action.yml + examples in lockstep, so the `version` input default in `action.yml` always matches the crate version under the same `vX.Y.Z` tag. This makes the GitHub Marketplace's auto-suggested `uses: ymw0407/cargo-chronoscope@v0.1.9` reference do the right thing — previously it pointed at a commit where `action.yml` defaulted to an older crate version, silently installing a stale binary.
- `action-v1` (moving) and `action-v1.0.x` (immutable) tags are kept working for existing users but marked legacy in the README. New workflows should pin to `@vX.Y.Z`. The `action-v1.0.x` immutable namespace is frozen at `action-v1.0.4` (= 0.1.8 era); future immutables are just the crate `vX.Y.Z` tag.

## [0.1.8] - 2026-05-05

### Fixed
- `cargo-chronoscope watch` and `record` now actually kill the cargo child process when the user presses `Ctrl-C` (or `q` in the TUI). Previously the `SupervisorHandle` returned by `spawn_build` was discarded, so the outer `CancellationToken` never reached the supervisor and cargo kept compiling silently in the background after the dashboard closed. ([#77](https://github.com/ymw0407/cargo-chronoscope/pull/77), closes [#60](https://github.com/ymw0407/cargo-chronoscope/issues/60))
- `fetch_baseline` now excludes compilations from failed and unfinalized builds. Pre-fix, the anomaly classifier's mean ± 2σ was contaminated by samples from `success = 0` and never-finalized builds, causing baseline drift and misclassified `slower` / `faster` verdicts on subsequent runs. ([#79](https://github.com/ymw0407/cargo-chronoscope/pull/79))

### Changed
- TUI: refactored `wait_for_exit_key` so the cancel-handling loop is testable via dependency injection on the key-reading callback, with a regression test that catches re-introductions of the pre-#34 bug. ([#55](https://github.com/ymw0407/cargo-chronoscope/pull/55), closes [#37](https://github.com/ymw0407/cargo-chronoscope/issues/37))

## [0.1.7] - 2026-05-05

### Added
- Windows (`x86_64-pc-windows-msvc`) prebuilt binary in the release matrix, plus a `cargo-binstall` override so `cargo binstall cargo-chronoscope` resolves the `.zip` archive on Windows. This is the recommended install path on Windows because source builds via `cargo install` hit Smart App Control / WDAC blocks on cargo's temp build-script `.exe` files. ([#64](https://github.com/ymw0407/cargo-chronoscope/pull/64))
- Forked-PR sticky perf-diff comments via a `workflow_run`-triggered companion workflow, both in this repo's CI and in the published action's example workflows. ([#59](https://github.com/ymw0407/cargo-chronoscope/pull/59))
- README hero GIF showing the four-command flow on ripgrep. ([#62](https://github.com/ymw0407/cargo-chronoscope/pull/62))

### Changed
- macOS Intel (`x86_64-apple-darwin`) is now cross-compiled from the Apple Silicon runner because `macos-13` Intel runners are no longer reliably allocated. The `0.1.6` release silently dropped this archive; `0.1.7` restores it. ([#64](https://github.com/ymw0407/cargo-chronoscope/pull/64))
- `CONTRIBUTING.md`, `.github/CONTRIBUTING.md`, and `CLAUDE.md` reframed the strict Integrator/Data/Realtime role-ownership rule as historical context; review routing now lives in `.github/CODEOWNERS` and external contributions to any module are explicitly welcome. ([9d4a91e](https://github.com/ymw0407/cargo-chronoscope/commit/9d4a91e))

### Removed
- The skeleton-phase `#![allow(dead_code)]` in `main.rs` and the items it was hiding: `BuildProfile::Custom`, `BuildEvent::CompilerMessage` + `MessageLevel`, `TuiState::set_build_id`, `BuildEvent::CompilationStarted.{kind, at}`, and the `kind` field on `ActiveCompilation` / `FinishedCompilation`. ([#61](https://github.com/ymw0407/cargo-chronoscope/pull/61))

## [0.1.6] - 2026-05-03

### Fixed
- `cargo-chronoscope watch`: pressing any key on the post-build dashboard no longer marks the run as interrupted. The dismiss keypress was sharing the same `CancellationToken` as the Ctrl-C handler, so it routed through `finalize_or_discard`'s "interrupted" branch and deleted the freshly recorded build. Fixes [#33](https://github.com/ymw0407/cargo-chronoscope/issues/33).

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

[Unreleased]: https://github.com/ymw0407/cargo-chronoscope/compare/v0.1.6...HEAD
[0.1.6]: https://github.com/ymw0407/cargo-chronoscope/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/ymw0407/cargo-chronoscope/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/ymw0407/cargo-chronoscope/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/ymw0407/cargo-chronoscope/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/ymw0407/cargo-chronoscope/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ymw0407/cargo-chronoscope/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ymw0407/cargo-chronoscope/releases/tag/v0.1.0
