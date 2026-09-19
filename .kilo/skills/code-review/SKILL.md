Review the current implementation against the documented task and project state.

Do NOT modify code unless I explicitly ask you to after the review.

Start by reading:
- AGENTS.md
- STATUS.md
- the relevant section of PLAN.md
- relevant project skills/instructions

Then inspect the implementation for the task described in STATUS.md.

Use repository tools efficiently:
- search before reading;
- inspect diffs and Git history first where useful;
- read only relevant source regions, interfaces, callers, and tests;
- avoid loading unrelated files or large logs.

Review for:

1. Correctness
   - Does the implementation actually satisfy the acceptance criteria?
   - Are there logic errors, edge cases, incorrect assumptions, or incomplete paths?

2. Architecture
   - Does it follow the documented architecture and invariants?
   - Did it introduce unnecessary abstractions, dependencies, duplication, or complexity?
   - Are subsystem boundaries still clean?

3. Kindle constraints
   - Consider ARMv7, limited RAM/CPU/storage, old Linux environment, and interruption/suspend behavior where relevant.
   - Identify anything that requires verification on real Kindle hardware.
   - Distinguish VERIFIED, INFERRED, and NEEDS EXPERIMENT.

4. Rust/Lua quality
   - Check error handling, resource ownership, APIs, SQLite usage, filesystem behavior, and failure recovery as applicable.
   - Look for unnecessary allocations or obviously expensive operations on constrained hardware.
   - For Lua/KOReader code, check lifecycle and integration assumptions.

5. Tests
   - Are the important behaviors actually tested?
   - Are tests testing behavior rather than implementation details?
   - Identify missing high-value tests.
   - Do not demand tests merely for coverage.

6. Security and robustness
   - Check handling of network input, feeds, HTML, paths, subprocesses, temporary files, SQLite data, and size/resource limits where relevant.

7. Scope
   - Identify unrelated changes or premature work.
   - Check that the implementation stayed within the intended task.

Run appropriate read-only inspection and validation commands where useful.
Do not claim something was tested unless you actually ran the command.

Prioritize findings by severity:

CRITICAL — correctness/data-loss/security issue that should block acceptance
HIGH     — significant bug or architectural problem
MEDIUM   — worthwhile issue that should probably be fixed
LOW      — minor improvement
NOTE     — observation or optional improvement

For every finding provide:
- severity;
- file and relevant symbol/location;
- concrete problem;
- why it matters;
- smallest reasonable fix.

Avoid speculative findings. If you cannot establish that something is actually
wrong, describe the uncertainty instead of presenting it as a defect.

Finish with:

## Acceptance criteria
For each acceptance criterion from STATUS.md:
- PASS
- FAIL
- NOT VERIFIED

with a short explanation.

## Validation
List commands/tests actually run and their results.

## Hardware verification
List anything that still needs testing on the Kindle.

## Recommended next action
Give the smallest set of changes needed before this task can be considered
complete. Do not implement them.

If there are no substantive issues, explicitly say so rather than inventing
improvements.