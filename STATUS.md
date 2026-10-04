# Current status

## Implementation

The plugin now schedules refresh deadlines through KOReader's `WakeupMgr`
instead of using the standalone Rust `powerd-daemon` path. Lua reads the
persisted Rust schedule, replaces its task by callback identity, dispatches a
bounded wake refresh only from that callback, then reads and schedules the next
deadline. The standalone listener, Upstart integration, service installer,
supervisor, and one-shot wake wrapper have been removed from source and from the
verified Kindle. The ARM backend binary was rebuilt without `powerd-daemon`.

The KOReader WakeupMgr callback now queues a fallback one-shot task before
starting network work. If Wi-Fi is disconnected, NetworkMgr enables it
asynchronously and the refresh starts only from its connected callback; no
blocking wait loop is used. The `schedule` command also exposes the persisted
refresh interval and last successful refresh alongside the deadline. The plugin
remains non-document-only and is loaded in both file-manager and reader modes.
LuaJIT syntax compilation and all 44 workspace tests passed after these
changes. Real Kindle wake/Wi-Fi behavior remains `NEEDS EXPERIMENT`.

On 2026-10-04 version 0.0.3 of the plugin and ARM backend was deployed to
`/mnt/us/koreader/plugins/rssreader.koplugin/`; its backend and Lua hashes were
verified at deployment. The existing database reported 12 feeds and a persisted
14,400-second refresh interval. Real WakeupMgr and Wi-Fi behavior remains
unverified.

Refresh status now presents the most recent ten sync runs in insertion order,
including timestamp, trigger, outcome, feed/article counters, errors, budget,
and duration. Schema v4 trims older run rows on migration and as new runs start
or finish. Last-success time is persisted outside the bounded history so pruning
does not erase it. Migration, retention, and last-success tests passed; LuaJIT
syntax, formatting, Clippy, and all 47 workspace tests passed. Version 0.0.4
was deployed on 2026-10-04 (backend SHA-256
`43f1c16f16c2fd8b142b9157e1375c5aec41cab189ab5f62372e8df8032ec4c0`, plugin
`c30f96db73ffe09fda5097bd844b20ebf85db7482f2e9f994a3f9deaa6ae9e77`). Device
verification reported backend version 0.0.4, successful schema v4 migration,
ten retained history entries, and all 12 feed rows intact. The persisted
14,400-second interval and last-success timestamp remain available. KOReader has
not been restarted with v0.0.4, so visual history rendering and scheduled
WakeupMgr execution remain unverified.

Validation for this change: LuaJIT bytecode compilation, `cargo fmt --all
--check`, Clippy, all 42 workspace tests, and ARM/QEMU smoke checks passed. The
rebuilt ARM binary was deployed; its hash matches the local artifact, and its
strings contain none of `powerd-daemon`, `lipc-wait-event`, `lipc-set-prop`,
`rtcWakeup`, or `deferSuspend`. A test package contains no retired wake module,
templates, or wrapper. The KOReader `WakeupMgr` implementation documents
callback-based `removeTasks` and invokes task callbacks only after the device
resume path validates the alarm. No local third-party plugins using this API
were present for pattern comparison.

The Rust backend, SQLite store, KOReader plugin, deployment scripts, and
validation tooling are implemented through the core offline-reader and
bounded-refresh scope.

Completed capabilities include:

- RSS/Atom parsing, feed validation, title persistence, OPML import, stable
  deduplication, validators, bounded HTTP, redirects, and retry/backoff state.
- Full-page article fetching per RSS entry, Readability extraction, allowlist
  sanitization, plain HTML without links, RSS-content fallback, and bounded
  EXIF-aware grayscale image embedding.
- Compressed self-contained article BLOBs, atomic disposable materialization,
  latest-article retention, physical-size maintenance, and cache bounds.
- SQLite schema v2 removes persisted read/unread columns and uses a 512 MiB
  database target; existing Kindle data migrated successfully.
- Deterministic sequential feed scheduling, refresh-run outcomes, status
  reporting, overlap locking, `Retry-After`, feed error reporting, and
  confirmed purge actions.
- Manual **Refresh now** runs unbounded until all due work completes; wake and
  unattended refreshes remain explicitly time-bounded. Response/image size and
  decoded-image limits keep an individual input bounded on Kindle.
- Article page and embedded-image downloads use a single bounded pipeline per
  feed; SQLite writes remain serialized and deterministic.
- KOReader RSS Reader menu with latest/per-feed views, paging, two-line entries
  using middle-dot separators between title, source, and date, feed management,
  refresh/status actions, external-link actions, and OPML startup import.
- SQLite schema migration removing persisted read/unread columns and increasing
  the database target from 256 MiB to 512 MiB.
- Scheduled background refresh through KOReader `Device.wakeup_mgr`, using the
  Rust backend's persisted absolute deadline. Only a WakeupMgr-validated alarm
  callback starts `refresh --reason wake`; Rust still applies bounded work,
  enabled-feed selection, and failure backoff.
- Host/QEMU/ARM validation, SSH device diagnostics, crash-boundary cache checks,
  and benchmarks.
- OPML smoke flow imports `assets/test-opml.opml`, verifies all 12 feeds, and
  retrieves articles from two stable feeds in that subscription set.
- Refresh article preparation releases the feed response before page downloads
  and processes one entry at a time to minimize Kindle peak memory. RSS feed
  content is committed before page work, then upgraded in place when full-page
  extraction succeeds. Persisted source kind distinguishes feed fallback from
  full-page content, allowing failed page downloads to be retried; full-device
  allocation behavior remains unverified.
- Refresh checks persisted article source kind before preparing entries, so
  full-page articles are skipped while feed-fallback articles are retried.
- Refresh lock recovery now detects a dead backend owner through `/proc` (while
  retaining the age fallback), so a SIGKILL/OOM cannot block later refreshes
  until the 15-minute stale-lock timeout; KOReader reports exit status 137 and
  the likely memory-pressure cause explicitly.

## Current evidence

Automated validation is passing:

- 41 Rust workspace tests.
- Rust formatting and Clippy.
- LuaJIT plugin syntax compilation.
- ARMv7 cross-build and QEMU smoke tests.
- Host interruption/cache validation and materialization benchmark.
- Kindle SSH backend/device diagnostics.
- Backend tests, formatting, and Clippy after the single-pipeline downloader
  change.
- Wake integration Lua syntax compilation, Rust workspace tests, and Rust
  formatting after the wake changes.
- Live RSS smoke tests passed for The Verge (10 entries) and Ars Technica (20
  entries).
- Unbounded manual refresh smoke test recorded `budget_s=0` and completed
  successfully.
- OPML smoke test imported 12 feeds and retrieved 30 articles.
- Fixture validation, Rust tests, formatting, and Clippy passed after the
  memory-bounded downloader change.
- Duplicate-download prevention passed workspace tests, Clippy, automated
  validation, and the materialization benchmark.
- Sequential scheduling and feed-first article persistence pass workspace tests,
  Clippy, formatting, and Lua syntax checks.
- Persisted feed/page source provenance and page-retry behavior pass workspace
  tests, Clippy, and formatting.
- Kindle database migration verified at schema version 3 with no `is_read` or
  `read_at` columns and `max_db_bytes=536870912`; 43 existing articles were
  preserved. Host migration tests now cover schema v4 history pruning.
- Persisted refresh deadline and due/backoff tests pass; the legacy powerd
  listener implementation has been removed.
- Dead-owner refresh-lock recovery tests and the updated KOReader error path
  pass focused validation.

The KOReader plugin entrypoint and ARM backend are deployed to the verified
Kindle as of 2026-10-02. Their SHA-256 hashes match local artifacts (backend
`cb48d178c35b7f438a9bf526b53055b54ba055cdbc602340e2bbef3f11fd2e99`, plugin
`bed1fb452dcf18e932ff016d6f64302c3485ab00b1f971b8609eff6be1b58440`). The
backend reports version `0.0.3`; the root filesystem is read-only and
`powerd.state` was `active` during verification. Host formatting, Clippy, 44
workspace tests, ARM/QEMU smoke tests, package creation, Lua syntax, and shell
syntax passed for the deployed state.

Verified device facts include Kindle Paperwhite 4 hardware, firmware/kernel
`4.1.15-lab126`, KOReader `v2026.07.1`, ARM backend execution, SQLite probes,
HTTPS probes, fixture initialization/materialization, and matching deployment
hashes.

## Needs experiment

The following require the installed KOReader/device and must not be inferred
from host or QEMU results:

- KOReader Lua SQLite behavior and filesystem locking/journal recovery.
- Base64 JPEG/PNG rendering, reader viewport, pagination, and sidecars/cache.
- Full feed/article UI interaction after restart.
- Wi-Fi readiness after wake, suspend/resume behavior, RTC scheduling, total
  awake duration, and battery impact.
- Real KOReader/Kindle `WakeupMgr` task execution and whether autosuspend
  naturally follows the bounded refresh.
- Re-run the interrupted manual refresh after the memory-limit fix and confirm
  peak RSS remains below the Kindle's available memory under the real feed set.
- Reproduce the original Kindle allocation failure with the single-entry
  refresh pipeline and confirm peak RSS under the full subscription set.

The retired listener had been active as PID 14974 with a 600-second budget.
Cleanup stopped it, removed `/etc/upstart/koreader-rss-wake.conf`, the data-dir
supervisor, old plugin installer/templates, the one-shot wrapper, the diagnostic
`probe.sqlite3`, and old `powerd.log`/`refresh.log` files. Verification reports
`Unknown job`, no listener process, and no listed experiment artifacts. The
production `rss.sqlite3` was preserved. `rtcWakeup` is write-only/unreadable via
the inspected LIPC property; no direct RTC or LIPC alarm manipulation was done.
Any device-level wake behavior remains `NEEDS EXPERIMENT`.

The latest device report also captured an OOM kill during manual refresh:
`rss-backend` PID 19037 was killed at 280,540 KiB anonymous RSS with swap full.
The persisted run was unfinished (`reason=1`, `budget_s=0`, `feeds_checked=0`);
the KOReader process was not the OOM victim. The wake callback was not involved.
Feed/page/image download limits are now 1/2/2 MiB; source images are rejected
before decode above 4 MP, decoder allocation is capped at 32 MiB, per-article
embedded image data is capped at 4 MiB, and rejected images have only their
`src` removed. Host tests, Clippy, and ARM/QEMU smoke checks passed, and the
updated backend was deployed. The production refresh was not rerun during this
debug session; hardware peak-RSS acceptance remains outstanding.

## Next actions

1. Restart KOReader to load v0.0.4 and verify the **Refresh status (last 10)**
   screen in file-manager and reader mode.
2. Perform genuine suspend/resume acceptance and verify the next `reason=2`
   history entry, Wi-Fi reconnection, and natural autosuspend.
3. Implement selector settings, larger extraction benchmarks, and
   refresh-transaction kill tests if needed after device observations.

## Project publishing setup

The initial workspace version `0.0.2` was used for the first published
milestone. Development was bumped to `0.0.3` on 2026-09-29; GitHub project
governance and publishing files include:

- `.github/workflows/ci.yml` runs formatting, Clippy, Rust tests, cache
  validation, Lua syntax checks, and ARMv7/QEMU smoke tests on pull requests
  and pushes to `main`.
- `.github/workflows/release.yml` builds the ARMv7 package for `v*` tags and
  publishes a downloadable ZIP archive to the GitHub release.
- `CONTRIBUTING.md` documents the fork-and-pull-request workflow.
- GitHub `main` branch protection is configured with required PR review, no
  force-push/direct-push access, and both CI jobs as required checks.
- `README.md` documents user installation and the verified Kindle evidence.

The next release tag should be `v0.0.4`; it must be created after these
changes are merged to `main` so the release workflow can publish the archive.

The two-worker memory-bounded backend `0.0.2` was deployed to the verified
Kindle at `/mnt/us/koreader/plugins/rssreader.koplugin/bin/rss-backend`.
ARMv7/QEMU smoke tests and Kindle device diagnostics passed, including
materialization and concurrent read probes. UI rendering and suspend/resume
acceptance remain manual.

The article list now prefixes source/date metadata with a middle dot and
separates source from date with another middle dot. The updated Lua plugin was
deployed on 2026-09-29; local and Kindle `main.lua` SHA-256 hashes match.
KOReader must reload/restart to display the updated rows.
