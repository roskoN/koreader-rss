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
- Validation passed: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (29 tests), `luajit -b koreader-plugin/main.lua /tmp/rssreader-main.luac`, `make test-arm` (cross-build plus QEMU ARM doctor/SQLite/materialization smoke), and `git diff --check`.

## Important discoveries

- Kindle access was previously verified at `/mnt/us/koreader`; device is ARMv7 i.MX6 SoloLite with NEON/VFPv3, 502 MiB RAM, 1072×1448/1448×1072 framebuffer modes, and 5.8 GiB free user storage. Static ARM backend probes passed on-device, including HTTPS example.com.
- Hardware KOReader SQLite/data-URI rendering, filesystem locking/journal interruption behavior, and wake/suspend behavior remain unverified.
- Refresh-all now processes due feeds fairly from the persisted cursor; physical SQLite behavior and backoff timing still need real-device measurement.

## Current task

Continue Milestones 4–7 in bounded slices: persisted scheduler state, validator-coherent refresh runs, cache/database size bounds, and complete feed-management UI. Validate on host and QEMU before any Kindle claims.

## Unresolved / evidence status

- `NEEDS EXPERIMENT`: restore Kindle SSH access and record the Milestone 0 evidence report; verify installed KOReader Lua SQLite API, Base64 JPEG/PNG rendering, reader content viewport, journal locking, and KOReader cache artifacts.
- `NEEDS EXPERIMENT`: wake hook, Wi-Fi readiness, suspend interaction, useful execution window, and battery impact.
- Extraction currently uses bounded article/body selection rather than the full Readability/ammonia pipeline from PLAN.md; feed-specific selectors are not implemented.

## Next task

Perform device-only KOReader feed UI/cache artifact and SQLite locking/journal experiments before any new Kindle claims.
