# Current status

## Implementation

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
- Fair persisted scheduling, refresh-run outcomes, status reporting, overlap
  locking, `Retry-After`, feed error reporting, and confirmed purge actions.
- Manual **Refresh now** runs unbounded until all due work completes; wake and
  unattended refreshes remain explicitly time-bounded.
- Article page and embedded-image downloads use a single bounded pipeline per
  feed; SQLite writes remain serialized and deterministic.
- KOReader RSS Reader menu with latest/per-feed views, paging, two-line entries,
  feed management, refresh/status actions, external-link actions, and OPML
  startup import.
- SQLite schema migration removing persisted read/unread columns and increasing
  the database target from 256 MiB to 512 MiB.
- Optional scheduled wake integration with explicit Wake refresh UI, versioned
  Upstart supervisor, powerd `rtcWakeup` arming during `readyToSuspend 1`,
  persistent absolute `next_refresh_at`, idempotent install/uninstall, bounded
  wake refresh/defer-suspend handling, and package/deploy support.
- One-shot unattended refresh wrapper, host/QEMU/ARM validation, SSH device
  diagnostics, crash-boundary cache checks, and benchmarks.
- OPML smoke flow imports `assets/test-opml.opml`, verifies all 12 feeds, and
  retrieves articles from two stable feeds in that subscription set.
- Refresh article preparation releases the feed response before page downloads
  and processes one entry at a time to minimize Kindle peak memory;
  full-device allocation behavior remains unverified.
- Refresh now queries persisted article dedupe keys before preparing entries,
  so already-downloaded entries and duplicate entries in one feed response do
  not trigger page, extraction, image, or compression work.
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
- Kindle database migration verified at schema version 3 with no `is_read` or
  `read_at` columns and `max_db_bytes=536870912`; 43 existing articles were
  preserved.
- Persistent wake deadline migration and due/backoff tests pass; Rust
  formatting, Clippy, workspace tests, Lua syntax, shell syntax, and package
  checks pass after the scheduled RTC supervisor change.
- Dead-owner refresh-lock recovery tests and the updated KOReader error path
  pass focused validation.

The current ARM backend and KOReader plugin have been deployed to the verified
Kindle at `/mnt/us/koreader/plugins/rssreader.koplugin/`, including `wake.lua`
and the versioned Upstart template. Post-deployment backend and package-path
checks passed.

The scheduled-wake build was redeployed to the same Kindle on 2026-09-20.
ARM/QEMU validation passed; device verification reported backend `0.0.2`,
schema version 3 scheduler state, and the new supervisor/config resources.
The optional Upstart job remains uninstalled (`status: Unknown job`).

The dead-owner lock recovery and KOReader exit-137 diagnostics were deployed
to the same Kindle on 2026-09-20. ARM release build, backend version check,
file deployment, and device diagnostics passed. KOReader restart and manual
refresh retry remain required for UI-level verification.

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
- Wi-Fi readiness after wake, suspend/resume behavior, RTC scheduling,
  `deferSuspend` semantics, total awake duration, and battery impact.
- Reproduce the original Kindle allocation failure with the single-entry
  refresh pipeline and confirm peak RSS under the full subscription set.

The one-shot wrapper is deployed as:

```text
/mnt/us/koreader/plugins/rssreader.koplugin/refresh-job.sh
```

The optional Upstart supervisor is not enabled automatically. It listens to
powerd `readyToSuspend` and `wakeupFromSuspend`, arms `rtcWakeup` through
powerd, and logs bounded diagnostics beside the database. Production
acceptance still requires repeated device cycles after explicitly enabling Wake
refresh; RTC and temporary suspend deferral remain device experiments.

## Next actions

1. Redeploy/restart KOReader so the dead-owner lock recovery and diagnostic
   message are active, then retry manual refresh.
2. Perform the suspend/resume wake acceptance procedure described in
   `docs/WAKE-INTEGRATION.md` and `docs/BACKGROUND-SYNC.md`.
3. Implement selector settings, larger extraction benchmarks, and
   refresh-transaction kill tests if needed after device observations.

## Project publishing setup

The workspace version is now `0.0.2`. GitHub project governance and publishing
files were added:

- `.github/workflows/ci.yml` runs formatting, Clippy, Rust tests, cache
  validation, Lua syntax checks, and ARMv7/QEMU smoke tests on pull requests
  and pushes to `main`.
- `.github/workflows/release.yml` builds the ARMv7 package for `v*` tags and
  publishes a downloadable ZIP archive to the GitHub release.
- `CONTRIBUTING.md` documents the fork-and-pull-request workflow.
- GitHub `main` branch protection is configured with required PR review, no
  force-push/direct-push access, and both CI jobs as required checks.
- `README.md` documents user installation and the verified Kindle evidence.

The next release tag should be `v0.0.2`; it must be created after these
changes are merged to `main` so the release workflow can publish the archive.

The two-worker memory-bounded backend `0.0.2` was deployed to the verified
Kindle at `/mnt/us/koreader/plugins/rssreader.koplugin/bin/rss-backend`.
ARMv7/QEMU smoke tests and Kindle device diagnostics passed, including
materialization and concurrent read probes. UI rendering and suspend/resume
acceptance remain manual.
