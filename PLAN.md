# RSS Reader Roadmap

This document is the forward-looking roadmap. Completed implementation and
device evidence are summarized in `STATUS.md`; this file intentionally avoids
chronological implementation notes.

## Architecture

```text
KOReader Lua plugin
        |
        | short-lived CLI commands
        v
Rust backend
        |
        v
SQLite authoritative store
        |
        +-- compressed, self-contained article HTML
        +-- feed, scheduler, read-state, and refresh-run metadata
```

The Rust backend owns network access, parsing, extraction, image processing,
compression, scheduling, retention, and all persistent writes. The Lua plugin
owns menus, bounded subprocess launches, read-only article navigation, and
opening disposable materializations. The Kindle's stock UI and power
management remain independent.

## Completed implementation

- RSS/Atom parsing, stable deduplication, validators, bounded HTTP, redirects,
  and failure/backoff scheduling.
- Full-page article fetching with Readability extraction and ammonia allowlist
  sanitization; RSS content is retained as a fallback.
- Plain extracted HTML with anchor elements and `href` attributes removed.
- Bounded grayscale, EXIF-aware image processing and Base64 embedding.
- Compressed SQLite article BLOBs and atomic disposable materialization.
- Persisted fair scheduling, refresh-run status, overlap locking, retention,
  cache limits, and incremental vacuum maintenance.
- Feed add/check/list/enable/disable/remove, title persistence, error
  reporting, confirmed purge controls, and OPML import from the plugin feeds
  directory.
- KOReader unread/all/per-feed article views, paging, refresh/status actions,
  two-line article rows, and external-link actions.
- Optional suspend/resume wake refresh integration with explicit KOReader
  enable/disable controls, Upstart supervision, and bounded `reason=wake`
  backend semantics.
- Host, QEMU, ARM, SSH diagnostics, interruption/cache automation, and
  materialization benchmarks.

## Remaining implementation work

### Automated

- Replace basic tag/ID/class selector matching with full CSS selector support.
- Add larger real-site extraction corpus tests and performance budgets.
- Add refresh transaction-boundary kill/recovery tests.
- Measure 100/1,000/10,000 article retention and physical SQLite behavior.
- Add a user-facing settings screen for selectors, retention, refresh budget,
  and cache limits.

### Device-only

- Verify KOReader SQLite runtime behavior, image rendering, reader viewport,
  sidecars/cache, and filesystem locking on the installed Kindle.
- Verify Wi-Fi readiness, suspend/resume, wake scheduling, total wake duration,
  and battery impact.
- Complete PW4 acceptance testing for the wake integration across repeated
  suspend/resume cycles, screensaver-only transitions, unavailable Wi-Fi,
  reboot persistence, and uninstall.

The device-only work is documented in `docs/MANUAL-VALIDATION.md` and
`docs/BACKGROUND-SYNC.md`. Do not claim Kindle-specific behavior from host or
QEMU results.

## Validation gates

Before device sessions:

```text
make test
make validate
make benchmark
make test-arm
make validate-device
```

After implementation changes, update `STATUS.md` with only durable results,
known limitations, and the exact next action.
