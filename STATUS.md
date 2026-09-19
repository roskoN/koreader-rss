# Current status

## Completed

- Milestone 0 bootstrap and on-device backend probes remain present; ARM Kindle inventory and static backend smoke evidence is recorded below.
- Feed adapter parses RSS/Atom with metadata preservation and stable GUID/URL/hash dedupe keys.
- Synchronous bounded HTTP refresh supports feed validators, idempotent article insertion, embedded-content fallback, webpage fallback extraction, per-entry failure records, and DEFLATE storage.
- Page and image requests are bounded. Article HTML strips dangerous block elements; remote images are resolved, decoded, resized, converted to grayscale JPEG/PNG, and embedded as Base64 data URIs with a 12-image limit.
- Article BLOBs have validated decompression and atomic disposable materialization through `materialize ID --cache DIR`; `mark ID read|unread` is available for the UI boundary.
- KOReader unread listing reads the application database, opens materialized articles with `ReaderUI:showReader`, and marks an article read only after the reader callback succeeds.
- Added `feed remove ID` and refresh-all mode (`refresh` without `--feed`), processing enabled feeds within the global budget.
- Refresh runs bounded read-article retention pruning (90-day and 5,000-row caps).
- Persisted round-robin scheduling now selects enabled due feeds from `app_state.scheduler_cursor`, advances after every attempted feed, and records interrupted/partial/completed `refresh_runs` with incremental counters.
- Materialization purges stale temporary files and enforces a disposable cache bound of 3 HTML files/32 MiB, retaining the requested article; refresh pruning now reads retention/article/size limits from SQLite settings and applies per-feed, total, and logical compressed-byte caps. Added `feed list` plus KOReader feed list/add/remove actions.
- Feed failures now use stable feed-derived jitter with bounded 15-minute/1-hour/4-hour/12-hour/24-hour backoff; pruning performs capped incremental freelist vacuum and exposes physical database byte measurement.
- Added headless `init-fixture-db` and `device-probe --cache DIR` commands, expanded CLI/retention tests, expanded ARM smoke coverage, and `docs/MANUAL-VALIDATION.md` for the remaining physical checks.
- Added disposable refresh overlap locks, bounded `Retry-After` handling for HTTP 429/503 responses, refresh-run pruning to the newest 20 rows, feed enable/disable CLI/UI controls, physical database-size maintenance, and additional HTML event-handler/JavaScript URL sanitization tests.
- Webpage fallback now uses `dom_smoothie` Readability followed by an explicit `ammonia` semantic HTML allowlist; the extraction fixture covers boilerplate removal and unsafe URL/script stripping.
- Validation passed: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (34 tests), `luajit -b koreader-plugin/main.lua /tmp/rssreader-main.luac`, `make test-arm` (cross-build plus QEMU doctor/SQLite/materialization/fixture/probe smoke), and `git diff --check`.
- Full extraction validation passed: `cargo test --workspace` (34 tests), Clippy, and `make test-arm`; the ARM release binary is 5,086,716 bytes (about 1.30 MB larger than the prior 3,791,876-byte build due to HTML parser/sanitizer dependencies).

## Important discoveries

- Kindle access was previously verified at `/mnt/us/koreader`; device is ARMv7 i.MX6 SoloLite with NEON/VFPv3, 502 MiB RAM, 1072×1448/1448×1072 framebuffer modes, and 5.8 GiB free user storage. Static ARM backend probes passed on-device, including HTTPS example.com.
- **VERIFIED on 2026-09-19:** `KOREADER_DIR=/mnt/us/koreader make kindle-test` completed on Kindle firmware/kernel `4.1.15-lab126`, model `Lab126 i.MX6SLL Board`, KOReader `v2026.07.1`, with 502 MiB RAM, 1072×1448 and 1448×1072 modes, `/mnt/us` on `fuse.fsp`, and static backend/HTTPS probes passing. A standalone KOReader LuaJIT SQLite probe failed because `package.loadlib` is unavailable outside the KOReader runtime; this does not establish plugin API failure.
- **VERIFIED on 2026-09-19:** `make deploy-plugin KOREADER_DIR=/mnt/us/koreader` installed the current two-file plugin atomically-by-file; local and remote SHA-256 hashes match. KOReader must be restarted before runtime menu probes can execute.
- **VERIFIED on 2026-09-19:** `make deploy-backend KOREADER_DIR=/mnt/us/koreader` installed the current ARM backend; local and remote SHA-256 hashes match. A disposable `/var/tmp` device fixture database was initialized, materialized, probed, and removed successfully (`schema_version=1`, `journal_mode=delete`, one article/cache file).
- **VERIFIED on 2026-09-19:** hardened backend and plugin were redeployed after refresh-lock, Retry-After, feed enable/disable, physical-size, and sanitization changes; the ARM backend hash matches the local release artifact.
- **VERIFIED on 2026-09-19:** Readability/ammonia ARM backend redeployed; local and Kindle SHA-256 hashes match (`ea3af1956c431621bd8a24f487e2bf1b76d5b0b735933628374dcf7cddac13b1`).
- Hardware KOReader SQLite/data-URI rendering, filesystem locking/journal interruption behavior, and wake/suspend behavior remain unverified.
- Refresh-all now processes due feeds fairly from the persisted cursor; physical SQLite behavior and backoff timing still need real-device measurement.

## Current task

Complete final automated hardening and then run the deployed KOReader/device validation checklist.

## Unresolved / evidence status

- `NEEDS EXPERIMENT`: restart KOReader and use the deployed plugin's “Initialize and query SQLite”, feed UI, and offline fixture/article actions; record SQLite API results, Base64 JPEG/PNG rendering, reader viewport, journal locking, and KOReader cache artifacts.
- `NEEDS EXPERIMENT`: wake hook, Wi-Fi readiness, suspend interaction, useful execution window, and battery impact.
- Feed-specific content/remove selectors, full corpus benchmarking, and kill-at-each-boundary interruption tests remain unimplemented.

## Next task

Run `docs/MANUAL-VALIDATION.md` after restarting KOReader; remaining implementation follow-ups are selector configuration, kill-boundary tests, corpus benchmarks, and wake integration measurements.
