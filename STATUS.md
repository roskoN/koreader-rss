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
- Article page and embedded-image downloads now use a bounded four-thread worker
  pool per feed; SQLite writes remain serialized and deterministic.
- KOReader RSS Reader menu with latest/per-feed views, paging, two-line entries,
  feed management, refresh/status actions, external-link actions, and OPML
  startup import.
- SQLite schema migration removing persisted read/unread columns and increasing
  the database target from 256 MiB to 512 MiB.
- Optional suspend/resume wake integration with explicit Wake refresh UI,
  versioned Upstart template, idempotent install/uninstall, bounded
  `--reason wake` refresh gating, and package/deploy support.
- One-shot unattended refresh wrapper, host/QEMU/ARM validation, SSH device
  diagnostics, crash-boundary cache checks, and benchmarks.
- OPML smoke flow imports `assets/test-opml.opml`, verifies all 12 feeds, and
  retrieves articles from two stable feeds in that subscription set.
- Refresh article preparation now uses two workers, a bounded result channel,
  and releases the feed response before page downloads to reduce Kindle peak
  memory; full-device allocation behavior remains unverified.

## Current evidence

Automated validation is passing:

- 38 Rust workspace tests.
- Rust formatting and Clippy.
- LuaJIT plugin syntax compilation.
- ARMv7 cross-build and QEMU smoke tests.
- Host interruption/cache validation and materialization benchmark.
- Kindle SSH backend/device diagnostics.
- Backend tests, formatting, and Clippy after the four-thread downloader change.
- Wake integration Lua syntax compilation, Rust workspace tests, and Rust
  formatting after the wake changes.
- Live RSS smoke tests passed for The Verge (10 entries) and Ars Technica (20
  entries).
- Unbounded manual refresh smoke test recorded `budget_s=0` and completed
  successfully.
- OPML smoke test imported 12 feeds and retrieved 30 articles.
- Fixture validation, Rust tests, formatting, and Clippy passed after the
  memory-bounded downloader change.
- Kindle database migration verified at schema version 2 with no `is_read` or
  `read_at` columns and `max_db_bytes=536870912`; 43 existing articles were
  preserved.

The current ARM backend and KOReader plugin have been deployed to the verified
Kindle at `/mnt/us/koreader/plugins/rssreader.koplugin/`, including `wake.lua`
and the versioned Upstart template. Post-deployment backend and package-path
checks passed.

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
- Wi-Fi readiness after wake, suspend/resume behavior, wake scheduling, total
  awake duration, and battery impact.
- Reproduce the original Kindle allocation failure with the new two-worker,
  bounded-result refresh and confirm peak RSS under the full subscription set.

The one-shot wrapper is deployed as:

```text
/mnt/us/koreader/plugins/rssreader.koplugin/refresh-job.sh
```

The optional Upstart listener is not enabled automatically. The Kindle exposes
powerd `wakeupFromSuspend`; production acceptance still requires repeated
device cycles after explicitly enabling Wake refresh. RTC scheduling remains a
separate experiment.

## Next actions

1. Run `docs/MANUAL-VALIDATION.md` after restarting KOReader.
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

The concurrent downloader and wake integration changes have now been deployed;
device diagnostics passed. UI rendering and suspend/resume acceptance remain
manual.
