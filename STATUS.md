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
  unread/read state, retention, physical-size maintenance, and cache bounds.
- Fair persisted scheduling, refresh-run outcomes, status reporting, overlap
  locking, `Retry-After`, feed error reporting, and confirmed purge actions.
- Article page and embedded-image downloads now use a bounded four-thread worker
  pool per feed; SQLite writes remain serialized and deterministic.
- KOReader RSS Reader menu with unread/all/per-feed views, paging, two-line
  entries, feed management, refresh/status actions, external-link actions, and
  OPML startup import.
- One-shot unattended refresh wrapper, host/QEMU/ARM validation, SSH device
  diagnostics, crash-boundary cache checks, and benchmarks.

## Current evidence

Automated validation is passing:

- 37 Rust workspace tests.
- Rust formatting and Clippy.
- LuaJIT plugin syntax compilation.
- ARMv7 cross-build and QEMU smoke tests.
- Host interruption/cache validation and materialization benchmark.
- Kindle SSH backend/device diagnostics.
- Backend tests, formatting, and Clippy after the four-thread downloader change.

The current ARM backend and KOReader plugin have been deployed to the verified
Kindle at `/mnt/us/koreader/plugins/rssreader.koplugin/`.

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

The one-shot wrapper is deployed as:

```text
/mnt/us/koreader/plugins/rssreader.koplugin/refresh-job.sh
```

It is not automatically scheduled. The Kindle exposes powerd `rtcWakeup`,
`rtcWakeup2`, and `wakeUp` interfaces, but setting `rtcWakeup` while active
returned `lipcPropErrInvalidState`; a suspend/wake experiment is required
before installing a persistent schedule.

## Next actions

1. Run `docs/MANUAL-VALIDATION.md` after restarting KOReader.
2. Perform the bounded powerd/RTC wake experiment described in
   `docs/BACKGROUND-SYNC.md`.
3. Implement selector settings, larger extraction benchmarks, and
   refresh-transaction kill tests if needed after device observations.

The concurrent downloader change was host-validated only; deployment was not
run.
