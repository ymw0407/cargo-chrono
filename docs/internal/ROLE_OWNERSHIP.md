# Role Ownership

Authoritative mapping of GitHub IDs → roles → owned modules. Used to route
issues, PR reviews, and "who do I ask about X" questions.

## Roles

| Role | GitHub | Display name | Modules owned |
|---|---|---|---|
| **Integrator** | [@ymw0407](https://github.com/ymw0407) | Minwoo Yun | `src/model/`, `src/cli/`, `src/supervisor/`, `src/parser/`, `src/main.rs`, `Cargo.toml` |
| **Data**       | [@yangfeiran20252335](https://github.com/yangfeiran20252335) | Yang Feiran | `src/persist/`, `src/diff/` |
| **Realtime**   | [@addbum421](https://github.com/addbum421) | (Realtime team) | `src/broker/`, `src/anomaly/`, `src/tui/` |

The Integrator is also responsible for `main.rs`, which is the only place
that wires modules from different roles together.

## Module dependency rules (hard)

```
model/                ← every module may import; model/ must not depend on any other src/ module
Data ↔ Realtime       not allowed in either direction
Realtime → Data       only via the BuildRepository trait (no SqliteRepository)
main.rs               the single assembly point for cross-role wiring
```

A PR that breaks these rules requires sign-off from the affected role
owner(s).

## Cross-role coordination

When a fix or feature unavoidably touches modules owned by another role:

1. Open a GitHub issue describing the change and tag the affected role
   owner.
2. Either ask that owner to take the change, or get explicit sign-off in
   the issue before opening the PR.
3. PR title/body must mention "(touches `<other-module>/`)" so reviewers
   notice.

Recent examples:
- [PR #10][pr10] — `fix/integrator/discard-cancelled-builds`: Integrator
  needed `BuildRepository::delete_build` (Data). Coordinated via [issue #9][i9].
- [PR #11][pr11] — `fix/data/concurrent-db-access`: pure Data work
  (busy_timeout, atomic migrations). Tracked via [issue #3][i3].

[pr10]: https://github.com/ymw0407/cargo-chronoscope/pull/10
[pr11]: https://github.com/ymw0407/cargo-chronoscope/pull/11
[i3]: https://github.com/ymw0407/cargo-chronoscope/issues/3
[i9]: https://github.com/ymw0407/cargo-chronoscope/issues/9

## Branch naming convention

```
<type>/<role>/<topic>
```

- `<type>` ∈ `feat`, `fix`, `refactor`, `test`, `docs`, `chore`
- `<role>` ∈ `integrator`, `data`, `realtime` — the role of the PR author,
  not necessarily of every file touched
- `<topic>` — short kebab-case description

Examples:
- `feat/realtime/anomaly-classifier`
- `fix/data/concurrent-db-access`
- `fix/integrator/discard-cancelled-builds`

Pure documentation branches that don't fit a single role may use
`docs/<topic>` (e.g. `docs/opensource-release-prep`).

## Commit message convention

Conventional Commits: `<type>(<scope>): <description>`

`<scope>` is the module name (`model`, `cli`, `supervisor`, `parser`,
`persist`, `diff`, `broker`, `anomaly`, `tui`, `main`). Multi-module
commits should be split.

Full rules: [`.github/COMMIT_CONVENTION.md`](../../.github/COMMIT_CONVENTION.md).
