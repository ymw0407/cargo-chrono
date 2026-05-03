# `docs/internal/` — historical and team-internal documents

These documents come from the planning phase of `cargo-chronoscope` (April–May 2026)
and from the post-completion record. They are checked in for context but are
**not user-facing documentation** — for that, see the top-level
[`README.md`](../../README.md).

| File | Purpose |
|---|---|
| [`DESIGN.md`](DESIGN.md) | Original architectural design — scenarios, modules, schema, and the three-role split. Korean. |
| [`CONCURRENCY.md`](CONCURRENCY.md) | Anticipated race conditions (12) and the mitigation strategy for each. Korean. Implementation checklist. |
| [`ONBOARDING.md`](ONBOARDING.md) | Day-1 checklist used by each role when joining the project. Korean. |
| [`AGENTS.md`](AGENTS.md) | Per-role guide for collaborating with AI coding assistants. Korean. |
| [`ROLE_OWNERSHIP.md`](ROLE_OWNERSHIP.md) | Mapping of contributors → roles → modules. The authoritative ownership reference. English. |
| [`PROJECT_HISTORY.md`](PROJECT_HISTORY.md) | Completion record: what was built, by whom, when. English. |

The team-internal conventions (commit message style, branching, role split,
module ownership rules) are still enforced via [`CLAUDE.md`](../../CLAUDE.md)
at the repo root.
