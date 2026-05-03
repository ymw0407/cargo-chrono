# cargo-chrono

> Cargo build performance observer — record, diff, and watch your Rust builds.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

`cargo-chrono` consumes Cargo's machine-readable build event stream, persists
each build to a local SQLite database, and gives you four ways to look at the
results: a real-time TUI dashboard while a build is running, a list of past
builds, a diff between any two builds, and a baseline-aware anomaly classifier
that flags crates compiling slower or faster than usual.

It addresses a corner of the Rust project's [2025 H2 goal "Prototype Cargo
build analysis"][rust-goal] from the outside — the part where an external tool
analyses historical trends.

[rust-goal]: https://rust-lang.github.io/rust-project-goals/2025h2/cargo-build-analysis.html

## Features

| Command | What it does |
|---|---|
| `cargo-chrono record [-- <cargo args>]` | Run a `cargo build`, record every compilation event to the local database. |
| `cargo-chrono watch [-- <cargo args>]`  | Same as `record`, plus a live ratatui dashboard showing active crates, anomaly verdicts, and CPU/memory. |
| `cargo-chrono ls [--last N]`            | List the most recent builds (default: 10). |
| `cargo-chrono diff <before> <after>`    | Compare two recorded builds: total time, per-crate movers, and side-by-side critical paths. |

Other things it does:

- **Anomaly classification** — every finished crate is compared against its
  historical mean ± 2σ and labelled `slower` / `faster` / `normal`. In the
  TUI, in-progress crates that have already exceeded the upper bound are
  flagged live.
- **Cancellation-aware recording** — pressing `q` or `Ctrl-C` mid-build
  discards the partial data instead of polluting your baselines with a
  half-recorded build.
- **Concurrent-safe storage** — multiple `cargo-chrono` processes can share
  the same database; SQLite's `busy_timeout` and a transactional migration
  serialise the few moments where it matters.

## Installation

### From source (current option)

```bash
git clone https://github.com/ymw0407/cargo-chrono.git
cd cargo-chrono
cargo install --path .
```

This puts `cargo-chrono` on your `PATH` (typically `~/.cargo/bin`).

A crates.io release will follow once the API stabilises.

## Quick start

```bash
# Pick a Rust project to observe.
cd ~/your-rust-project

# Watch a build live.
cargo clean
cargo-chrono watch

# Or record without a UI, then inspect later.
cargo clean
cargo-chrono record

cargo-chrono ls
cargo-chrono diff 1 2
```

`cargo-chrono` runs `cargo build` in the current directory and stores its
data in `./.cargo-chrono/history.db` (SQLite, WAL mode). Add this directory
to your `.gitignore`:

```gitignore
.cargo-chrono/
```

## Usage

### `record` — store a build for later analysis

```bash
cargo-chrono record                   # cargo build
cargo-chrono record -- --release      # cargo build --release
cargo-chrono record -- -p my_crate    # cargo build -p my_crate
```

Anything after `--` is forwarded verbatim to `cargo build`. On success it
prints `Build #N recorded.` On `Ctrl-C` the partial row is deleted and you
get `Build interrupted — not recorded.` instead.

### `watch` — record + live TUI dashboard

```bash
cargo-chrono watch
cargo-chrono watch -- --release
```

```
┌─ cargo-chrono ───────────────────────────────────────┐
│ Build #5 (release) • commit abc1234 • elapsed 0:28   │
│ 142 crates compiled                                  │
├─ Active compilations ────────────────────────────────┤
│  ▶ serde_derive    12.4s   ⚠ slower                  │
│  ▶ syn              8.1s   · normal                  │
├─ Recently finished (last 5) ─────────────────────────┤
│  ✓ proc-macro2      5.8s   ↓ faster                  │
├─ System  [q] quit  [Ctrl-C] interrupt ───────────────┤
│  CPU: 75.5%   Memory: 4.0 GiB / 16.0 GiB             │
└──────────────────────────────────────────────────────┘
```

Exit keys: `q`, `Q`, or `Ctrl-C`. After the build finishes, the final frame
stays on screen until you press a key — convenient when the build was a fast
cache hit.

> Run from a real terminal (iTerm2 / Terminal.app), not an IDE-integrated
> terminal, for the best raw-mode behaviour. The dashboard restores the
> terminal on panic via a RAII guard, but `reset` will recover it manually
> if anything ever leaks.

### `ls` — list builds

```bash
cargo-chrono ls
cargo-chrono ls --last 30
```

```
ID     Started              Profile  Duration   Status
------------------------------------------------------------
#3     2026-05-03T01:31:14  release  1:32       ok
#2     2026-05-03T01:29:48  release  1:28       ok
#1     2026-05-03T01:13:41  dev      0:42       FAIL
```

### `diff` — compare two builds

```bash
cargo-chrono diff 1 2
```

```
Build #1 → Build #2
  Total: 0:42 → 1:28 (+0:46, +109.5%)

  ▲ syn               1.20s → 2.45s (+1.25s, +104.2%)
  ▼ proc-macro2       0.80s → 0.55s (-0.25s, -31.3%)
  + serde-derive (new) 0.92s
  - lazy_static (gone) 0.05s
  … 137 crates unchanged

Critical path: 14 → 11 nodes (-3)

    #  before              after
  ───  ──────────────────  ──────────────────
    1  cfg_if              memchr
    2  equivalent          bytes
    3  pin_project_lite    autocfg
    4  unicode_ident       shlex
    5  foldhash            foldhash             ✓
    ...

  removed from path: scopeguard, version_check, ryu
```

Markers:
- `▲` / `▼` — crate got slower / faster
- `+` / `-` — crate added / removed from this build
- `✓` — same crate at the same critical-path position
- `…` — unchanged crates collapsed into a count

## How it works

```
              ┌──────────┐    JSON     ┌─────────┐
   cargo ────►│Supervisor│────lines───►│ Parser  │
   (stdout +  └──────────┘   (mpsc)    └─────────┘
    stderr)                                 │
                                       BuildEvent
                                            │
                  ┌─────────────────────────┴─────────────┐
                  │                                       │
            ┌─────▼─────┐                          ┌──────▼─────┐
            │   Broker  │ (watch mode only)        │  Persister │
            └─────┬─────┘                          └──────┬─────┘
                  │                                       │
              ┌───┴───┐                                ┌──▼──┐
              │ TUI   │                                │ DB  │
              └───────┘                                └─────┘
```

- **Supervisor** spawns `cargo build --message-format=json-render-diagnostics`
  and merges its stdout (JSON) and stderr (`Compiling foo v0.1.0` progress
  lines) into a single line stream.
- **Parser** turns that stream into typed `BuildEvent`s. The `Compiling` lines
  give per-crate start times; the `compiler-artifact` JSON gives per-crate end
  times.
- **Persister** writes each event to SQLite via the `BuildRepository` trait.
- **Broker** (watch mode only) fans events out to multiple subscribers — the
  persister and the TUI — without backpressuring either.
- **TUI** consumes events at ~60 fps, looks up baselines via the repository,
  and renders the dashboard.
- **Anomaly** module classifies durations against a baseline (mean ± `n·σ`).

## Database schema

A single SQLite file at `<workspace>/.cargo-chrono/history.db`:

```
builds
  id, started_at, finished_at, commit_hash, cargo_args,
  profile, success, total_duration_ms

crate_compilations
  id, build_id, crate_name, crate_version, kind,
  started_at, finished_at, duration_ms
```

WAL mode is enabled. `crate_compilations.build_id` references `builds.id`
(no `ON DELETE CASCADE` — `delete_build` removes both rows in one
transaction).

You can query the database directly with `sqlite3 .cargo-chrono/history.db`
if you want.

## Status

Active development. The CLI surface above is what works today; expect new
flags and formats as the tool evolves.

Known gaps:

- Linux/Windows are not yet tested in CI (developed on macOS aarch64).
- `cargo --timings` integration is not yet exposed (cargo's own per-crate
  timing report could feed the same database).
- `anomaly` thresholds are not configurable from the CLI (hardcoded to 2σ).

See [issues](https://github.com/ymw0407/cargo-chrono/issues) for the
prioritised list.

## Contributing

PRs welcome. Before opening one, please run:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

Conventional Commits format (`feat(scope): …`, `fix(scope): …`). Module
scopes: `model`, `cli`, `supervisor`, `parser`, `persist`, `diff`, `broker`,
`anomaly`, `tui`, `main`.

For the historical design notes, role split, and concurrency analysis from
the planning phase, see [`docs/internal/`](docs/internal/).

## License

MIT — see [LICENSE](LICENSE).
