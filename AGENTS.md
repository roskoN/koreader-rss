# AGENTS.md

## Project

Offline-first RSS/news reader for a jailbroken Kindle Paperwhite 4 (10th generation, 2018).

The project consists of:

* a Rust backend for fetching, processing, storing, and materializing articles
* a KOReader Lua plugin for the user interface
* SQLite as the authoritative persistent store
* supporting Kindle integration for background synchronization, packaging,
  deployment, and device experiments

The primary target is constrained ARM Kindle hardware. Optimize for correctness,
simplicity, reliability, low memory usage, low total energy consumption, and
fast perceived latency.

The normal Amazon Kindle UI and reading experience must remain intact.

Read `STATUS.md` before starting work.

Use:

* `PLAN.md` for the project roadmap
* `STATUS.md` for current state and handoff
* `docs/` for architecture and technical decisions
* project skills for specialized workflows in `.kilo/skills`

Repository state and project documentation are authoritative over assumptions
from previous agent sessions.

## Core architecture

Unless an explicit documented decision changes it, preserve these principles:

* Rust implements the native backend.
* Lua implements the thin KOReader UI/integration layer.
* SQLite is the authoritative persistent data store.
* Persistent article content and images belong in SQLite.
* Temporary materialized documents may exist outside SQLite but must be
  disposable and reconstructable.
* The backend should normally execute as short-lived commands rather than as a
  permanent daemon.
* Background synchronization should be resumable and bounded.
* Kindle power management should be respected rather than replaced.
* Stock Kindle functionality must not depend on this application.
* Hardware-specific behavior must be verified on real hardware before being
  treated as established.

Do not silently change these architectural constraints.

If evidence strongly suggests a better architecture, document the evidence and
proposed change before implementing a broad migration.

## Work discipline

Work on one bounded task at a time.

Before editing:

1. Read `STATUS.md`.
2. Read the relevant portion of `PLAN.md` when necessary.
3. Identify the exact current task.
4. Identify its acceptance criteria.
5. Check repository state.
6. Search the repository before reading broadly.
7. Inspect only relevant implementation, interfaces, documentation, and nearby
   tests.
8. Form a short implementation plan.

Do not start unrelated work or automatically continue into the next roadmap
task.

Prefer the smallest coherent change that satisfies the acceptance criteria.

Avoid:

* unrelated refactoring
* speculative abstractions
* opportunistic cleanup
* formatting unrelated files
* premature optimization
* architecture changes without evidence

Follow existing project conventions.

## Autonomous work

For long-running tasks, continue autonomously through normal implementation,
debugging, testing, and iteration.

A failed command, failed test, incorrect assumption, or unsuccessful
implementation attempt is not by itself a reason to stop.

Use:

```text
hypothesis -> experiment -> evidence -> decision
```

Resolve ordinary engineering choices using, in order:

1. existing project conventions
2. documented decisions
3. existing abstractions
4. tests and observed behavior
5. authoritative upstream documentation/source
6. small reversible experiments
7. the simplest reasonable implementation

Ask for human input only when progress genuinely requires:

* credentials or access unavailable to the agent
* explicit authorization
* physical interaction that cannot be performed remotely
* a destructive or irreversible operation
* a materially ambiguous product/architecture decision
* unavailable required hardware or external service
* clarification that cannot reasonably be resolved from repository evidence

For extended unattended work, follow the long-running-task skill.

## Context discipline

Context is limited and should be treated as a scarce resource.

Prefer:

```text
search -> targeted read -> edit -> targeted validation -> inspect diff
```

Rules:

* Search before reading.
* Prefer targeted source regions over whole files.
* Never read the entire repository unless genuinely necessary.
* Do not repeatedly read unchanged content.
* Keep command output small and filtered.
* Do not dump complete logs when the relevant error can be isolated.
* Use project documentation instead of reconstructing established decisions.
* Use Git history when historical context is needed.
* Record durable discoveries in repository documentation rather than relying on
  conversational memory.

If a task unexpectedly requires substantially more context, stop expanding
indiscriminately.

Instead:

1. summarize what has been established
2. record it in `STATUS.md` or appropriate documentation
3. decompose the remaining work
4. continue with the smallest useful next step

A fresh agent should be able to resume from repository state alone.

## Tool use

Prefer tools over manual exploration.

For repository discovery:

* exact symbol/string/error -> `rg`
* structural code pattern -> `sg` / ast-grep
* file discovery -> `fd`
* repository overview -> `tree`
* history/context -> `git log`, `git show`, `git blame`

Prefer targeted commands over broad dumps.

Use specialized project skills when applicable rather than reconstructing their
workflow from scratch.

For potentially long-running commands:

* use finite timeouts where appropriate
* distinguish slow execution from a hung process
* capture output when useful
* inspect relevant sections rather than repeatedly printing everything
* terminate genuinely stuck processes
* preserve diagnostic information before retrying

Never repeatedly rerun an unchanged failing command without obtaining new
evidence.

## Implementation principles

Keep changes focused on the current task.

Prefer:

* existing abstractions over new abstractions
* simple data flow over clever indirection
* explicit state over hidden behavior
* bounded resource usage
* idempotent/resumable operations
* deterministic behavior where practical
* transactional persistent-state changes

Avoid new dependencies without a concrete benefit.

Before adding a dependency, consider:

* whether an existing dependency already solves the problem
* maintenance status
* licensing
* ARMv7 support
* libc/runtime requirements
* cross-compilation complexity
* binary-size impact
* memory impact
* TLS/native-library requirements

Do not silently change documented architectural decisions.

## Kindle constraints

Assume the target Kindle has substantially fewer CPU, memory, storage, and
energy resources than the development machine.

Optimize the whole workload rather than individual microbenchmarks.

Important resources include:

* wall-clock execution time
* total CPU time
* peak RSS memory
* allocations
* network bytes
* storage bytes
* device awake time

For background work, minimizing total device awake time may be more important
than minimizing instantaneous CPU utilization.

Avoid:

* unnecessary permanent processes
* polling when events are available
* large dependency stacks
* unbounded queues
* loading many complete articles or images simultaneously
* unnecessary filesystem scans
* excessive temporary files
* repeated decompression/parsing
* unnecessary writes

Stream data where practical.

Process large images one at a time unless measurements justify otherwise.

## Persistent data

SQLite is the authoritative persistent store.

Persistent state should not depend on loose files remaining synchronized with
database rows.

Use database constraints and transactions to maintain consistency.

Where appropriate:

* enable foreign-key enforcement
* use `ON DELETE CASCADE` for dependent article resources
* make synchronization resumable
* make repeated operations idempotent
* preserve successfully completed work across interruption

Temporary materialized article files must be disposable and reconstructable
from persistent state.

Do not create persistent filesystem state that can silently become orphaned
from SQLite without a documented reason.

## RSS/network behavior

Network work must be bounded and interruption-safe.

Use finite:

* connection timeouts
* read/request timeouts
* retry counts
* concurrency
* queues

Retry transient failures reasonably, preferably with bounded backoff.

Do not retry permanent failures indefinitely.

Use conditional HTTP requests such as ETag and Last-Modified where appropriate.

Synchronization should make useful partial progress and survive:

* network loss
* process termination
* Kindle suspend
* malformed feeds/articles
* individual server failures

One problematic feed or article should not prevent unrelated feeds from making
progress.

When scheduling work across feeds, preserve fairness rather than allowing one
large feed backlog to monopolize a bounded synchronization window.

## KOReader integration

Keep the Lua layer thin.

Do not move expensive parsing, network processing, image processing, or other
heavy backend work into Lua without a compelling measured reason.

The KOReader layer should primarily handle:

* menus
* article/feed lists
* user actions
* launching backend operations
* displaying state
* opening materialized documents
* reader integration

Avoid loading large result sets into Lua when pagination or targeted queries can
be used.

Do not assume undocumented KOReader behavior.

Inspect upstream KOReader source or verify experimentally when necessary.

## Kindle system integration

Treat Amazon's Kindle services and power-management infrastructure as shared
system components.

Do not casually:

* replace stock services
* modify stock UI components
* alter Amazon databases
* overwrite system files
* disable power management globally
* interfere with Amazon RTC scheduling
* leave permanent keep-awake state enabled

Prefer documented/existing Kindle mechanisms such as LIPC/powerd integration
when appropriate.

Temporary experimental device state should be restored after the experiment
when practical.

Stock Kindle reading must continue working independently of this project.

## Evidence levels

For uncertain Kindle, KOReader, firmware, or hardware behavior, explicitly
distinguish:

* `VERIFIED` — confirmed from authoritative source or direct relevant
  experiment
* `INFERRED` — supported by evidence but not directly confirmed
* `NEEDS EXPERIMENT` — requires testing, usually on actual hardware

Do not present inferred behavior as verified.

Host or QEMU behavior does not verify Kindle-specific behavior.

Cross-compilation success does not verify runtime compatibility.

Real-device experiments should record:

* device/firmware when relevant
* exact command/build tested
* expected result
* observed result

Promote behavior to `VERIFIED` only when the evidence supports it.



The primary development environment may differ from the target.

Distinguish clearly between:

### Host

Development machine / WSL.

Use for:

* normal editing
* unit tests
* static analysis
* formatting
* most backend tests
* representative fixtures

### QEMU

Use where useful for:

* ARM execution
* ABI problems
* architecture-specific behavior
* cross-built binary smoke tests

Do not use QEMU results as evidence for Kindle-specific:

* power management
* E-Ink behavior
* Amazon services
* Wi-Fi behavior
* KOReader integration
* suspend/resume

### Real Kindle

Use for final verification of hardware/system-dependent behavior.

Treat device access as relatively expensive. Validate as much as possible
locally before deploying.

## Remote-device experiments

Before running an experiment on the Kindle:

1. Define what question the experiment answers.
2. Build and validate locally where possible.
3. Cross-compile successfully.
4. Keep deployment narrowly scoped.
5. Define expected observations.
6. Define recovery if the experiment fails.

Prefer application/homebrew-controlled paths over modifications to system
partitions.

Do not leave:

* test processes
* keep-awake settings
* temporary services
* debug configurations
* unnecessary SSH exposure
* large temporary files

after experiments unless intentionally required.

## Performance work

Do not optimize blindly.

Before substantial optimization:

1. establish a representative workload
2. measure a baseline
3. identify the suspected bottleneck
4. change one meaningful factor
5. measure again
6. compare relevant metrics
7. retain the optimization only when evidence supports it

For this project, relevant metrics include:

* end-to-end article processing latency
* synchronization completion time
* compression ratio
* compression/decompression CPU time
* image processing time
* peak RSS
* database latency
* network traffic
* total Kindle awake duration

Prefer real Kindle measurements for decisions materially affected by its
hardware.

Do not extrapolate modern desktop/ARM benchmark numbers as if they were PW4
measurements.

## Validation

After modifying code:

1. Run the smallest relevant tests/checks.
2. Fix failures caused by the change.
3. Run formatting/linting when appropriate.
4. Inspect `git diff`.
5. Inspect `git status`.
6. Check for unintended changes.

During iteration, prefer narrow validation.

Examples include:

* one unit test
* one test module
* affected Rust crate
* Lua syntax check
* focused integration test
* specific QEMU smoke test

Before declaring the task complete, run the broader validation required by the
task and repository.

For Rust, where applicable:

```sh
cargo fmt --check
cargo clippy
cargo test
```

Use repository-specific commands when they exist.

Never claim a command, test, benchmark, deployment, or experiment succeeded
unless it was actually run.

Never hide failing validation.

Do not weaken a valid test merely to obtain a passing result.

## Failure handling

A failure is evidence.

When something fails:

1. isolate the relevant error
2. reproduce it if useful
3. classify the likely cause
4. form a concrete hypothesis
5. run the smallest experiment that tests it
6. adjust based on evidence
7. rerun relevant validation

Avoid chains of speculative changes.

If an external dependency blocks one part of a task, continue independent work
when useful and safe.

Do not declare a blocker until reasonable alternatives have been exhausted.

## Destructive operations

Do not autonomously perform destructive or difficult-to-reverse actions unless
explicitly authorized.

Examples include:

* deleting user data
* discarding unknown working-tree changes
* rewriting Git history
* force-pushing
* rotating credentials
* modifying production infrastructure
* irreversible Kindle modifications
* deleting large amounts of persistent application data

Prefer reversible experiments.

Never use `git reset --hard`, broad `git clean`, or equivalent destructive
commands merely to obtain a clean working tree when unrelated changes exist.

## Git discipline

Preserve unrelated user changes.

Before editing, inspect:

```sh
git status --short
```

Before committing:

1. inspect `git diff`
2. inspect `git status`
3. run the smallest required validation
4. confirm only intended changes are included

Keep commits focused and reviewable.

* One logical change per commit.
* Do not mix unrelated refactoring.
* Do not mix formatting-only changes with functional work unless necessary.
* Do not commit secrets.
* Do not commit debug output.
* Do not commit generated artifacts unless the repository intentionally tracks
  them.
* Do not commit unrelated user changes.

Use imperative commit messages describing the outcome, for example:

```text
Add article cache pruning
Persist conditional HTTP metadata
Handle interrupted image downloads
```

If a bounded task is incomplete, do not create a misleading completion commit.

Coherent completed milestones may be committed as checkpoints during
long-running work when appropriate.

## Documentation

Documentation should capture durable information, not session narration.

Update architecture/decision documentation when:

* a significant design decision changes
* a non-obvious constraint is established
* an important hardware behavior is verified
* future agents would otherwise need to rediscover expensive information

Do not document speculative behavior as fact.

Keep experimental observations clearly labeled.

## STATUS.md

`STATUS.md` is the durable handoff between agent sessions.

Before ending completed or interrupted work, update it with:

* current taskde
* what was completed
* relevant files changed
* tests/checks actually run and results
* important decisions/discoveries
* evidence status for hardware-specific findings
* unresolved issues
* blockers
* exact next task/action

Keep it concise.

Do not use `STATUS.md` as a chronological work log.

It must contain enough information for a fresh agent to continue without access
to previous conversation context.

During long-running unattended work, update `STATUS.md` at meaningful
milestones rather than risking loss of substantial state.

## Blocked work

When genuinely blocked:

1. preserve useful completed work
2. run all validation still possible
3. leave the repository coherent
4. update `STATUS.md`
5. identify exactly what is blocked
6. record evidence establishing the blocker
7. state the precise human/external action required
8. state the exact next action after unblocking

Avoid vague handoffs such as:

> Needs more investigation.

Prefer:

> `NEEDS EXPERIMENT`: verify whether `powerd` preserves the requested wake
> behavior across deep suspend on PW4 firmware 5.18.1.1.1. Backend implementation
> and host tests are complete. Next action: deploy build X to the Kindle, run
> command Y, allow the device to suspend for Z minutes, and inspect log A.

## Definition of done

A task is complete only when:

* its acceptance criteria are satisfied
* the implementation is coherent and appropriately scoped
* relevant tests/checks were actually run
* known failures are resolved or explicitly documented
* hardware-dependent claims have appropriate evidence labels
* `git diff` was reviewed
* unrelated changes were not introduced
* documentation was updated where necessary
* `STATUS.md` contains an accurate handoff
* the repository is left in a usable state

Implementation that merely appears correct is not sufficient.

## End-of-task report

Keep the final report concise.

State:

* what changed
* validation performed and results
* anything remaining `NEEDS EXPERIMENT`
* known limitations/blockers
* exact next task if work remains

Do not reproduce the entire contents of `STATUS.md`.