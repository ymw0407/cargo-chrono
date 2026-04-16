# Test Fixtures

This directory holds sample data for integration and unit tests.

## Planned fixtures

- `sample_output.jsonl` — Raw output captured from
  `cargo build --message-format=json-render-diagnostics` on a real project.
  **Day 1 task**: Run the command on any medium-sized crate and save its stdout here.

## How to capture

```bash
cargo build --message-format=json-render-diagnostics 2>/dev/null > tests/fixtures/sample_output.jsonl
```

Each line is a self-contained JSON object emitted by Cargo.
The Parser module uses these lines as input.
