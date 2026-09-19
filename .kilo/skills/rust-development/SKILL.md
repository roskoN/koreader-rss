---
name: rust-development
description: Implement, test, debug, or optimize the Rust backend for the offline-first Kindle RSS reader.
---

# Rust backend

The backend fetches and processes feeds, persists authoritative state in SQLite,
and materializes reader-ready article content for the KOReader Lua plugin.

Search for the owning module, public boundary, and nearest test before reading
implementation details. Make the smallest coherent change; preserve existing
error, async, and configuration conventions.

## Boundaries and invariants

- SQLite is authoritative; do not make files or plugin state a competing source
  of truth.
- Persistent article content is stored as compressed, self-contained HTML BLOBs
  with embedded image data where applicable.
- Keep backend/plugin contracts explicit and backward-compatible unless the
  task deliberately changes both sides.
- Prefer bounded/streaming work where practical; Kindle CPU, memory, and storage
  are constrained.
- Avoid adding dependencies without a concrete benefit.

## Rust tools

Choose the smallest useful validation or diagnostic tool.

- `cargo check` — fast compile/type validation.
- `cargo test` — focused or complete test execution.
- `cargo nextest` — efficient test execution when useful.
- `cargo fmt --check` — formatting validation.
- `cargo clippy` — lint and correctness checks.
- `cargo bloat` — investigate binary-size contributions.
- `/usr/bin/time -v` — measure runtime and peak memory.
- `hyperfine` — compare performance of repeatable commands.
- `gdb` — native debugging when normal diagnostics are insufficient.
- `strace` — investigate filesystem, process, network, or syscall behavior.

Prefer focused commands first. Do not run expensive repository-wide checks when
a targeted check answers the question.

## Validation

For implementation changes, normally use:

`focused test -> cargo check -> cargo fmt --check -> git diff`

Run broader `cargo test`/`cargo nextest` and `cargo clippy` when warranted by the
scope of the change.

For performance-sensitive work, measure before and after. Do not claim an
optimization from intuition alone.

For binary-size work, measure the release build and use `cargo bloat` to identify
actual contributors before changing dependencies or implementation.

Host validation does not prove ARMv7 or Kindle compatibility. Cross-compilation,
QEMU, deployment, and hardware-specific validation belong to the Kindle
development workflow.