---
name: codebase-navigation
description: Efficiently explore the Kindle RSS repository with minimal context. Use for locating code, behavior, decisions, tests, dependencies, or history before a focused change.
---

# Codebase navigation

Keep context bounded. Read `STATUS.md` first, then search before opening files.

## Route the search

Choose the cheapest tool that answers the question.

- Exact symbol, string, error, table, setting, or reference: `rg -n`.
- Filename or extension: `fd` or `rg --files`.
- Structural Rust or Lua code pattern: `sg` (ast-grep).
- Repository structure: `tree -L N`.
- Repository size/languages: `tokei`.
- File/output size before reading: `wc`.
- JSON data/configuration: `jq`.
- Why code exists or changed: targeted `git log`, `git show`, or `git blame`.
- Conceptual location when exact names are unknown: semantic codebase search if
  available, then narrow with `rg` or `sg`.

Semantic search results are discovery hints. Verify them against actual source
before modifying code.

## Read efficiently

After locating relevant code, read the smallest useful region.

Prefer:
- targeted file reads or `sed -n 'START,ENDp'`
- `head`/`tail` for file boundaries
- surrounding implementation plus its public interface, caller, and nearest test

Avoid:
- reading whole files when a region is sufficient
- broad recursive directory listings
- reading many candidate files before searching
- repeatedly reading unchanged content
- unfiltered build/test logs
- large generated files

If output may be large, narrow or measure it before placing it in context.

## Search progression

For exact questions:

`rg -> targeted read`

For structural questions:

`sg -> rg references -> targeted read`

For conceptual questions:

`semantic search -> rg/sg -> targeted read`

For historical questions:

`git log -> git show/blame -> targeted read`

Do not escalate to a broader search until the narrower search fails.

## Working set

Start with at most:
- current status/task
- one implementation area
- its relevant boundary/interface
- its nearest test

Expand only when a concrete unanswered question requires it.

If understanding the task starts requiring many files or multiple subsystems,
stop expanding context. Record:
- the unanswered question
- what has already been established
- which additional subsystem is required

Then split the investigation/task if practical.

## Before changing code

State the smallest acceptance criteria.

Inspect the relevant implementation and test before editing.

After editing:
1. Run the narrowest meaningful validation.
2. Inspect `git diff`.
3. Check for unintended changes.

Do not begin the next roadmap task or refactor unrelated code.