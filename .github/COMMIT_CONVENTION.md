# Commit Convention

This project follows [Conventional Commits](https://www.conventionalcommits.org/).

## Format

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

## Type

| Type | Meaning |
|------|---------|
| `feat` | New feature |
| `fix` | Bug fix |
| `refactor` | Code change that does not alter behaviour |
| `test` | Adding or fixing tests |
| `docs` | Documentation only |
| `chore` | Build, CI, dependencies, or other miscellany |
| `perf` | Performance improvement |
| `style` | Formatting / whitespace (no behavioural change) |

## Scope

`<scope>` is the module name. Each scope has an owning role.

| Scope | Owning role |
|-------|-------------|
| `model` | Integrator |
| `cli` | Integrator |
| `supervisor` | Integrator |
| `parser` | Integrator |
| `persist` | Data |
| `diff` | Data |
| `broker` | Realtime |
| `anomaly` | Realtime |
| `tui` | Realtime |
| `main` | Integrator |
| `ci` | shared |
| `docs` | shared |

A commit that touches multiple modules should usually be split. If it
genuinely cannot be split, pick the dominant scope and call out the rest
in the commit body.

## Examples

```
feat(supervisor): implement cargo process spawn and stdout streaming

Spawns `cargo build --message-format=json-render-diagnostics` as a child
process and streams stdout line-by-line through a bounded mpsc channel.

Closes #12
```

```
fix(persist): handle empty builds table on first run

The list_builds query was failing when the builds table had no rows.
Added a check for empty results before attempting to map rows.
```

```
test(anomaly): add edge case tests for zero std_dev

Covers the case where all historical compilation times are identical,
resulting in std_dev = 0.
```

```
refactor(model): rename CompilationRecord to CrateCompilation

Aligns naming with the design document terminology.

BREAKING CHANGE: CompilationRecord is now CrateCompilation
```

## Rules

1. **Subject line ≤ 50 characters**, lowercase, no trailing period.
2. **Body wraps at 72 characters** (optional).
3. **Breaking changes** must be flagged with `BREAKING CHANGE:` in the
   footer.
4. **Issue linking**: use `Closes #N`, `Fixes #N`, or `Refs #N` in the
   footer.
5. One logical change per commit.

## Branch naming

```
feat/<role>/<topic>      # new feature        — feat/data/sqlite-crud
fix/<role>/<topic>       # bug fix            — fix/realtime/tui-crash-on-exit
refactor/<role>/<topic>  # refactor           — refactor/integrator/parser-error-handling
docs/<topic>             # documentation      — docs/update-design
test/<role>/<topic>      # tests              — test/anomaly/edge-cases
chore/<topic>            # CI / config / etc. — chore/rename-to-cargo-chronoscope
```

Examples:
- `feat/data/sqlite-crud`
- `feat/realtime/broker-publish-loop`
- `fix/integrator/supervisor-kill-signal`
- `docs/opensource-release-prep`
