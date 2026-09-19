# AGENTS.md

## Project

Offline-first RSS/news reader for a jailbroken Kindle.

The project consists of:
- a Rust backend for fetching, processing, storing, and materializing articles
- a KOReader Lua plugin for the user interface
- SQLite as the authoritative persistent store

Read `STATUS.md` before starting work.
Use `PLAN.md` for the project roadmap and `docs/` for architecture and
technical decisions.

## Work discipline

Work on one bounded task at a time.

Before editing:
1. Read `STATUS.md`.
2. Identify the exact current task and its acceptance criteria.
3. Search the repository before reading broadly.
4. Inspect only the relevant implementation, interfaces, and nearby tests.
5. Form a short implementation plan.

Do not start unrelated work or the next task.

Prefer the smallest coherent change that satisfies the task.
Avoid unrelated refactoring and speculative architecture changes.
Follow existing project conventions.

## Context discipline

Context is limited and should be treated as a scarce resource.

- Search before reading.
- Prefer targeted source regions over whole files.
- Never read the entire repository unless genuinely necessary.
- Do not repeatedly read unchanged content.
- Keep command output small and filtered.
- Do not dump full logs when the relevant error can be isolated.
- Use project documentation instead of reconstructing established decisions.
- Use Git history when historical context is needed.

If a task requires substantially more context than expected, stop expanding
context. Record what was discovered and propose a smaller decomposition rather
than loading large portions of the repository.

## Tool use

Prefer tools over manual exploration.

For repository discovery:
- exact symbol/string/error -> `rg`
- structural code pattern -> `sg` (ast-grep)co
- file discovery -> `fd`
- repository overview -> `tree`
- history/context -> `git log`, `git show`, `git blame`

Prefer this workflow:

`search -> targeted read -> edit -> targeted validation -> git diff`

Use specialized project skills when applicable rather than improvising their
workflow from scratch.

## Implementation

- Keep changes focused on the current task.
- Reuse existing abstractions before creating new ones.
- Avoid new dependencies without a concrete benefit.
- Do not silently change documented architectural decisions.
- Optimize for correctness and simplicity before premature optimization.
- Keep constrained Kindle hardware in mind.

For uncertain Kindle/KOReader behavior, distinguish:

- `VERIFIED` — confirmed from source, documentation, or experiment
- `INFERRED` — supported by evidence but not directly confirmed
- `NEEDS EXPERIMENT` — requires testing on actual hardware

Do not present inferred behavior as verified.

## Validation

After modifying code:

1. Run the smallest relevant tests/checks.
2. Run formatting/linting when appropriate.
3. Inspect `git diff`.
4. Check for unintended changes.

Never claim a command, test, or experiment succeeded unless it was actually run.

QEMU/cross-platform tests do not establish that Kindle-specific behavior works.
Hardware-dependent behavior must ultimately be verified on the real Kindle.

## Handoff

Before ending a completed or interrupted task, update `STATUS.md` with:

- what was completed
- relevant files changed
- tests/checks run and their results
- important decisions or discoveries
- unresolved issues
- the exact next task

Keep `STATUS.md` concise.

It must contain enough information for a fresh agent session to continue
without access to the previous conversation.

## Commit discipline

Keep commits focused and reviewable.

- Make one logical change per commit.
- Do not mix refactoring, formatting-only changes, or unrelated fixes into a task commit.
- Before committing, inspect `git diff` and `git status`.
- Run the smallest relevant validation and record the result.
- Use an imperative commit message that describes the outcome, e.g. `Add article cache pruning`.
- Do not commit generated files, secrets, debug output, or unrelated user changes.
- If the task is incomplete, do not create a misleading “complete” commit; either finish the bounded task or clearly label the partial state.