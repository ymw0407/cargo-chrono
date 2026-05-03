# Project History

A factual record of what was built, by whom, and when. Kept in chronological
order — most recent at the bottom. Use [`git log`](https://github.com/ymw0407/cargo-chronoscope/commits/main)
for the full per-commit detail.

## Phase 1 — Skeleton (2026-04-26 → 2026-04-28)

Initial repository setup, planning documents, and scaffolding.

| Date | What | Who |
|---|---|---|
| 2026-04-26 | Project skeleton: Cargo.toml, module layout, contract tests as a spec | @ymw0407 |
| 2026-04-26 | Design docs (DESIGN.md, CONCURRENCY.md, ONBOARDING.md, AGENTS.md) — all in Korean, planning phase | @ymw0407 |
| 2026-04-28 | `parser`: `run_parser` implementation — JSON → BuildEvent stream | @ymw0407 |
| 2026-04-28 | `supervisor`: `spawn_build` with cancellable line streaming | @ymw0407 |

## Phase 2 — Module implementations (2026-04-29 → 2026-05-03)

Each role implemented their owned modules.

| Date | What | Role | Who |
|---|---|---|---|
| 2026-04-29 | `broker`: `publish_loop` (fan-out + dead subscriber cleanup) | Realtime | @ymw0407 |
| 2026-04-29 | `persist`: SQLite-backed `BuildRepository` | Data | @yangfeiran20252335 |
| 2026-04-30 | `parser`: scaffold + integrator wiring | Integrator | @ymw0407 |
| 2026-04-30 | `tui`: dashboard skeleton (state, render, system_monitor, run_tui) | Realtime | @addbum421 |
| 2026-05-03 | `data`: `run_persister`, `compute_diff`, critical path analysis | Data | Minwoo Yun |
| 2026-05-03 | `tui`: full dashboard implementation | Realtime | @addbum421 |

## Phase 3 — Bug fixes & UX polish (2026-05-03)

Real-world testing on the `deno` repository surfaced a cluster of
bugs that all shipped on the same day.

| Date | What | Scope | Who |
|---|---|---|---|
| 2026-05-03 | `tui`: cache baselines, handle Ctrl-C in raw mode | Realtime | @ymw0407 |
| 2026-05-03 | `cli`: clippy 1.95 `unnecessary_sort_by` | Cross-cutting | @ymw0407 |
| 2026-05-03 | `cli`: collapse Unchanged crates, sort by impact, ▲/▼ markers | Integrator | @ymw0407 |
| 2026-05-03 | `tui`: keep dashboard on screen after fast cache-hit builds | Realtime | @ymw0407 |
| 2026-05-03 | `supervisor`: pipe stderr alongside stdout (precondition for next) | Integrator | @ymw0407 |
| 2026-05-03 | `parser`: anchor `CompilationStarted` on cargo's `Compiling X v…` stderr line — fixes "all durations are 0" bug | Integrator | @ymw0407 |
| 2026-05-03 | `cli`: side-by-side critical path diff with ✓ markers | Integrator | @ymw0407 |
| 2026-05-03 | `cli`: print full critical path (no truncation — load-bearing metric) | Integrator | @ymw0407 |

## Phase 4 — Concurrency hardening & open-source preparation (2026-05-03)

Tracked via GitHub issues from this point forward.

| Date | What | Issue / PR | Who |
|---|---|---|---|
| 2026-05-03 | [#9][i9] filed: Ctrl-C builds are recorded as `FAIL` and pollute baselines | issue #9 | @ymw0407 |
| 2026-05-03 | `persist`: `BuildRepository::delete_build` for cancelled builds | [PR #10][pr10] | @ymw0407 |
| 2026-05-03 | `main`: discard build record when user cancels with Ctrl-C / `q` | PR #10 | @ymw0407 |
| 2026-05-03 | [#3][i3] revisited: SQLITE_BUSY race on concurrent `cargo-chronoscope` processes | issue #3 | @ymw0407 |
| 2026-05-03 | `persist`: 5s `busy_timeout` on connection open | [PR #11][pr11] | @ymw0407 |
| 2026-05-03 | `persist`: atomic migrations with `PRAGMA user_version` guard | PR #11 | @ymw0407 |
| 2026-05-03 | Open-source release prep: MIT LICENSE, English README, planning docs moved to `docs/internal/` | this PR | @ymw0407 |

[i3]: https://github.com/ymw0407/cargo-chronoscope/issues/3
[i9]: https://github.com/ymw0407/cargo-chronoscope/issues/9
[pr10]: https://github.com/ymw0407/cargo-chronoscope/pull/10
[pr11]: https://github.com/ymw0407/cargo-chronoscope/pull/11

## Status snapshot at release prep

What works:

- 4 commands: `record`, `watch`, `ls`, `diff`
- Real per-crate timing (parser bug fix)
- Live TUI with anomaly classification (slower / faster / normal / unknown)
- Side-by-side critical path comparison
- Cancellation-aware recording
- Concurrent-process-safe DB access

What does not yet work / is on the backlog:

- Linux/Windows CI matrix (developed on macOS aarch64)
- `cargo --timings` ingestion
- CLI flags for anomaly thresholds
- crates.io release
- Schema versioning for forward-incompatible changes (current is v1)

Test count at release prep: **128 unit + integration tests**, all passing
under `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`.

## How this document is maintained

Append a new row to the appropriate phase table whenever a meaningful chunk
of work lands on `main`. "Meaningful" = a closed issue, a merged PR, or a
distinct phase boundary. Don't list individual bug-fix commits unless they
fix a regression or unblock something.

For brand-new phases (e.g. major version, new subsystem), open a new
`## Phase N — …` section with a short paragraph of context above the table.
