# Current status

## Completed

- Milestone 0 repository/bootstrap work is present: Cargo workspace, static-oriented release profile, backend `doctor`, HTTPS probe, SQLite probe, atomic offline HTML fixture with embedded grayscale PNG/JPEG, KOReader probe plugin, packaging/deployment scripts, and QEMU smoke script.
- Added the first Milestone 1 feed adapter in `backend/src/feed.rs`: RSS/Atom parsing through `feed-rs`, metadata preservation, URL canonicalization, and GUID/URL/hash dedupe keys with RSS and Atom tests.
- Native validation passed on 2026-09-19: `make test` (fmt, clippy `-D warnings`, 9 Rust tests) and release `doctor` (SQLite 3.53.2).
- Kindle access is now verified: `/mnt/us/koreader` is the installed path; the device is an ARMv7 i.MX6 SoloLite with NEON/VFPv3, 502 MiB RAM, 1072×1448/1448×1072 8-bit framebuffer modes, and 5.8 GiB free user storage. The static ARM backend was deployed and `--version`, `doctor`, SQLite initialization, fixture materialization, and HTTPS (`example.com`, HTTP 200) passed on-device.

## Important discoveries

- No Kindle authentication or hardware measurements are available in this session, so Milestone 0's real-device exit gate remains open.
- Existing backend has schema/store primitives and bounded feed HTTP client, but no feed parser, canonical dedupe adapter, refresh command, or article-content wrapper.

## Current task

Integrate the feed adapter with `HttpClient` and `Store` behind a bounded `refresh --feed` command, then exercise RSS and Atom fixtures end-to-end.

## Unresolved

- Restore Kindle SSH access and record the Milestone 0 evidence report before claiming its exit gate.
- Verify the installed KOReader Lua SQLite API and Base64 JPEG/PNG rendering on hardware.
- Direct standalone `luajit` loading of `lua-ljsqlite3` fails because `package.loadlib` is absent; this must be retried inside the KOReader runtime/plugin before declaring Lua SQLite integration failed.

## Next task

After refresh integration, add canonical HTML wrapping and DEFLATE storage for embedded feed content.
