# KOReader Offline RSS Reader — Architecture and Milestone Plan

Status: planning only. No application code exists yet and no broad implementation should begin before Milestone 0 passes.

## Evidence and status key

- **VERIFIED** — confirmed in the local environment or inspected source.
- **INFERRED** — recommended from confirmed evidence, but not yet measured on the target Kindle.
- **NEEDS EXPERIMENT** — must be checked on the installed Kindle/KOReader before depending on it.

Research baselines:

- **VERIFIED:** `/home/rosti/git_repos/rss_kindle` was empty and was not a Git repository. Temporary reference clones were removed after inspection; this plan is the only created file.
- **VERIFIED:** Feedbook was inspected at commit `5e4891c65f2940171f23e797ea1d7b1d98dfb383` (v0.2.3).
- **VERIFIED:** KOReader was inspected at commit `d9788bdf90d0242cfaa14c95e29aa0ed2389761c`. This proves current upstream behavior, not the installed Kindle build.
- **VERIFIED:** local Rust is 1.98.1; `armv7-unknown-linux-musleabihf` is installed; `cross` 0.2.5, Docker, and QEMU 10.0.13 are available; QEMU exposes `cortex-a9`.
- **NEEDS EXPERIMENT:** `ssh kindle` resolves to `root@192.168.1.109:2222`, but this session had no private key and the device rejected authentication. No device fact was independently measured in Plan mode.

## A. Architecture

**INFERRED — chosen architecture**

```text
KOReader Lua plugin (menus, read-only queries, process launch, ReaderUI)
            |                    ^
            | tiny CLI calls     | short SQLite reads
            v                    |
Rust backend (single writer, network, parsing, extraction, images,
              compression, scheduling, retention, materialization)
            |
            v
one SQLite database
  feeds + scheduler + status + settings
  articles(metadata + compressed self-contained HTML BLOB)
  transient item failure records

SQLite BLOB --materialize--> bounded disposable HTML cache --open--> KOReader
```

Rules:

1. SQLite is the only authoritative persistent application store.
2. A stored article is complete: metadata plus one non-null compressed BLOB containing one UTF-8, self-contained HTML5 document.
3. Images are inside that HTML as Base64 `data:` URIs; there is no persistent image table or asset tree.
4. Rust is the only writer. Lua opens short-lived read-only connections if Milestone 0 proves the binding and cross-process locking.
5. Network and CPU work are sequential in the MVP. No Tokio, daemon, worker pool, or permanent service.
6. No network or image work occurs while a SQLite write transaction is open.
7. KOReader owns rendering. The plugin owns navigation, not typography or pagination.
8. Temporary lock, materialized HTML, and `.tmp` files are disposable mechanics, never authoritative state.

## B. Feedbook reuse analysis

### Confirmed useful parts

**VERIFIED:** Feedbook uses:

- `feed-rs 2.3` for RSS/Atom (`src/feed.rs`).
- `dom_smoothie 0.17` Readability plus `dom_query` (`src/scraper.rs`).
- `ammonia 4` allowlist sanitization (`src/sanitize.rs`).
- `rusqlite` with bundled SQLite (`src/cache.rs`).
- `image 0.25` with selected JPEG/PNG/GIF/WebP/ICO features (`src/images.rs`).
- conditional `ETag` and `Last-Modified` requests and 304 handling (`src/feed.rs`, `src/cache.rs`).
- relative image URL resolution, scheme filtering, failed-image removal, per-host throttling, configurable extraction/removal selectors, bounded feed/article/image concurrency, and stage timings.
- ARMv7 cross-build flags for Cortex-A9, NEON, VFPv3 and Thumb-2 in `.cargo/config.toml`; its README documents `cross` builds and Kindle packaging.

**INFERRED — reuse as dependencies/approaches:** adopt `feed-rs`, `dom_smoothie`, `ammonia`, `rusqlite(bundled)`, `image` feature selection, `url`, `chrono`, and `thiserror`, subject to binary/RAM benchmarks. Reuse the tested architectural ideas of conditional feed fetches, custom extraction selectors, URL resolution, failed-image removal, sequential SQLite access, and pipeline timings.

### What must be adapted, not copied

**VERIFIED:** Feedbook's feed adapter keeps only the first entry link, title, first author, and date. It does not carry GUID, embedded full content, or summary. Our adapter must use more of `feed-rs`'s model and implement a stable feed-scoped dedupe key.

**VERIFIED:** Feedbook always downloads article pages. Our `auto` policy must prefer good embedded feed content and avoid that request.

**VERIFIED:** Feedbook's extraction path and sanitizer are EPUB/XHTML-oriented. Its sanitizer removes useful inline wrappers and performs EPUB-check fixups. We should retain the proven Readability + allowlist sequence, but use a smaller semantic HTML5 allowlist tailored to KOReader.

**VERIFIED:** Feedbook finds and rewrites images into separate EPUB assets. Our equivalent stage resolves and downloads images, then writes Base64 data URIs into the same article document.

**VERIFIED:** Feedbook stores uncompressed article `TEXT` plus a deduplicated image table, uses `WAL`/`NORMAL`, and batches a whole feed pipeline before committing. Those choices conflict with the required atomic article BLOB, one-writer/short-transaction model, and conservative Kindle journaling.

### What not to reuse

- **VERIFIED:** `rbook`, EPUB/KEPUB generation, covers/favicons, font rendering, Kobo span injection, filesystem output, TOML-centric application configuration, fingerprints of generated ebooks, and library-rescan scripts are out of scope.
- **VERIFIED:** Feedbook's `wreq` + full Tokio/futures stack pulls BoringSSL bindings, HTTP/2, Brotli, zstd, and a large async graph. It is proven as a Feedbook choice but conflicts with the desired synchronous, small backend. Do not adopt it without a measured reason.
- **VERIFIED:** Feedbook passes through already-small color JPEG/PNG bytes, preserves SVG, only constrains width, and does not fix orientation or grayscale. That is unsuitable for this Kindle.
- **VERIFIED:** Feedbook uses parallel image optimization (`imagequant`, `oxipng` with Rayon) in its dependency set. Do not include these in the MVP.
- **VERIFIED:** Feedbook caches failed extraction as an empty article. That violates our complete-article invariant; failures stay outside `articles` and are retried/backed off.

### Licensing

**VERIFIED:** the inspected Feedbook repository has no root `LICENSE`, no package `license` field, and no source SPDX statement. Only its vendored `patches/boring2` has an Apache/MIT license. Therefore Feedbook source, tests, and assets must be treated as not licensed for copying or adaptation.

**Decision:** reuse independently licensed crates and high-level facts/ideas only. Do not copy Feedbook code, tests, prose, or assets unless the author supplies an explicit license. Record crate licenses and notices with `cargo-deny` or equivalent before release.

**VERIFIED:** KOReader's inspected `COPYING` is AGPL-3.0. Write the plugin from scratch against its public internal APIs. The safest distribution choice is AGPL-3.0-or-later for the plugin; obtain legal review before choosing a different backend/repository license.

## C. KOReader integration design

**VERIFIED in upstream source:** 

- Smallest plugin skeleton: `plugins/hello.koplugin/{_meta.lua,main.lua}` — `WidgetContainer:extend`, `init`, `registerToMainMenu`, `addToMainMenu`.
- Closest structural feature reference: `plugins/newsdownloader.koplugin/main.lua` — submenu construction, `NetworkMgr:runWhenOnline`, `KeyValuePage`, input/confirm dialogs, settings/data directories.
- Article-list/opening reference: `plugins/calibre.koplugin/search.lua` closes its menu and calls `ReaderUI:showReader(path)`.
- `ReaderUI:showReader` is documented in `frontend/apps/reader/readerui.lua` as the only safe way to create a reader. It accepts an `after_open_callback`, invoked after `ReaderReady`; use that to mark read only after a successful open.
- HTML/HTM/XHTML are registered with crengine in `frontend/document/credocument.lua`.
- `DataStorage:getDataDir()`, `getSettingsDir()`, and `getFullDataDir()` are established paths; KOReader pre-creates `cache`, `data`, `plugins`, and `settings` subdirectories.
- `lua-ljsqlite3` is used by KOReader's statistics, vocabulary, cover-browser, exporter, and cache code. `frontend/cachesqlite.lua` demonstrates BLOB binding, prepared queries, and open/close-per-operation.
- `frontend/ui/trapper.lua` provides `wrap`, `dismissablePopen`, and `dismissableRunInSubprocess`. `dismissablePopen` keeps the UI responsive only until output becomes readable; it does not reliably cancel the spawned command.
- `util.shell_escape(args)` safely forms a shell command from a fixed argument array.
- Upstream Kindle KUAL metadata points at `/mnt/us/koreader`, but the installed location is not yet verified.

**INFERRED plugin layout:** `rssreader.koplugin` contains Lua files plus `bin/rss-backend`; database lives under `DataStorage:getDataDir()/data/rssreader/`; disposable HTML lives under `DataStorage:getDataDir()/cache/rssreader/`. The plugin always passes absolute `--db` and `--cache` paths.

**NEEDS EXPERIMENT:** installed KOReader version/API compatibility, actual plugin path, executable permission/mount behavior, SQLite version/compile options, ReaderUI opening behavior, data-URI rendering, and KOReader history/sidecar/cache artifacts.

## D. SQLite schema and indexes

**INFERRED — schema v1** (Unix UTC seconds; integer booleans; basic SQL only so an older Lua SQLite can read it):

```sql
CREATE TABLE app_state (
    singleton             INTEGER PRIMARY KEY CHECK (singleton = 1),
    scheduler_cursor      INTEGER NOT NULL DEFAULT 0,
    last_maintenance_at   INTEGER
);

CREATE TABLE settings (
    singleton                 INTEGER PRIMARY KEY CHECK (singleton = 1),
    default_refresh_s         INTEGER NOT NULL DEFAULT 21600,
    retention_days            INTEGER NOT NULL DEFAULT 90,
    max_articles_per_feed     INTEGER NOT NULL DEFAULT 500,
    max_articles_total        INTEGER NOT NULL DEFAULT 5000,
    max_db_bytes              INTEGER NOT NULL DEFAULT 268435456,
    cache_max_files           INTEGER NOT NULL DEFAULT 3,
    cache_max_bytes           INTEGER NOT NULL DEFAULT 33554432
);

CREATE TABLE feeds (
    id                    INTEGER PRIMARY KEY,
    source_url            TEXT NOT NULL UNIQUE,
    effective_url         TEXT,
    site_url              TEXT,
    title                 TEXT,
    enabled               INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),
    content_policy        INTEGER NOT NULL DEFAULT 0, -- 0 auto, 1 feed, 2 webpage
    content_selector      TEXT, -- one optional CSS selector in v1
    remove_selector       TEXT, -- one optional pre-extraction CSS selector in v1
    include_images        INTEGER NOT NULL DEFAULT 1 CHECK (include_images IN (0,1)),
    refresh_interval_s    INTEGER NOT NULL DEFAULT 21600,
    max_articles          INTEGER,
    schedule_order        INTEGER NOT NULL UNIQUE,
    http_etag             TEXT,
    http_last_modified    TEXT,
    http_expires_at       INTEGER,
    last_attempt_at       INTEGER,
    last_checked_at       INTEGER,
    last_success_at       INTEGER,
    next_due_at           INTEGER NOT NULL DEFAULT 0,
    backoff_until         INTEGER,
    failure_count         INTEGER NOT NULL DEFAULT 0,
    last_error            TEXT,
    created_at            INTEGER NOT NULL
);

CREATE TABLE articles (
    id                    INTEGER PRIMARY KEY,
    feed_id               INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
    dedupe_key            TEXT NOT NULL,
    guid                  TEXT,
    url                   TEXT,
    title                 TEXT NOT NULL,
    author                TEXT,
    published_at          INTEGER,
    sort_at               INTEGER NOT NULL,
    fetched_at            INTEGER NOT NULL,
    source_kind           INTEGER NOT NULL, -- 1 embedded, 2 webpage
    is_read               INTEGER NOT NULL DEFAULT 0 CHECK (is_read IN (0,1)),
    read_at               INTEGER,
    compression_codec     INTEGER NOT NULL, -- 1 zlib-deflate, 2 zstd, 3 brotli
    content_format        INTEGER NOT NULL, -- 1 HTML5 UTF-8
    storage_version       INTEGER NOT NULL,
    uncompressed_size     INTEGER NOT NULL,
    compressed_size       INTEGER NOT NULL,
    content_blob          BLOB NOT NULL,
    UNIQUE(feed_id, dedupe_key),
    CHECK (compressed_size = length(content_blob))
);

CREATE TABLE entry_failures (
    feed_id               INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
    dedupe_key            TEXT NOT NULL,
    url                   TEXT,
    title                 TEXT,
    attempt_count         INTEGER NOT NULL,
    last_attempt_at       INTEGER NOT NULL,
    next_retry_at         INTEGER NOT NULL,
    source_token          TEXT, -- ETag/Last-Modified/body token that produced the item
    is_permanent          INTEGER NOT NULL DEFAULT 0 CHECK (is_permanent IN (0,1)),
    last_error            TEXT NOT NULL,
    PRIMARY KEY(feed_id, dedupe_key)
) WITHOUT ROWID;

CREATE TABLE refresh_runs (
    id                    INTEGER PRIMARY KEY,
    reason                INTEGER NOT NULL, -- manual/wake/CLI
    started_at            INTEGER NOT NULL,
    finished_at           INTEGER,
    budget_s              INTEGER,
    feeds_checked         INTEGER NOT NULL DEFAULT 0,
    new_articles          INTEGER NOT NULL DEFAULT 0,
    failed_feeds          INTEGER NOT NULL DEFAULT 0,
    outcome               INTEGER,
    last_error            TEXT
);

CREATE INDEX idx_feeds_due
    ON feeds(enabled, next_due_at, schedule_order);
CREATE INDEX idx_articles_newest
    ON articles(sort_at DESC, id DESC);
CREATE INDEX idx_articles_unread
    ON articles(is_read, sort_at DESC, id DESC);
CREATE INDEX idx_articles_feed
    ON articles(feed_id, sort_at DESC, id DESC);
CREATE INDEX idx_entry_failures_retry
    ON entry_failures(next_retry_at);
CREATE INDEX idx_refresh_runs_started
    ON refresh_runs(started_at DESC);
```

Use keyset pagination `(sort_at,id)` rather than large `OFFSET`s. Always select metadata columns explicitly so list queries never load `content_blob`.

Deduplication key precedence: non-empty feed GUID/Atom ID (`g:`), else conservatively normalized canonical item URL (`u:`), else SHA-256 of normalized title + publication time + a bounded content prefix (`h:`). It is feed-scoped. Strip URL fragments and normalize scheme/host/default port, but retain query parameters to avoid conflating distinct articles.

Migrations run only in Rust inside a transaction using `PRAGMA user_version`; set an application ID. Do not use `STRICT`, generated columns, JSON SQL, `RETURNING`, or other newer schema features until the installed Lua SQLite is measured.

### Journaling/tuning

**VERIFIED from SQLite documentation:** WAL adds `-wal`/`-shm`, shared-memory behavior, checkpoints, and checkpoint starvation; rollback journal is the default. WAL's concurrency advantage is unnecessary with one writer and short Lua reads. A 2026-documented WAL reset bug also affects many older SQLite versions under particular multi-connection write/checkpoint races.

**INFERRED default:** `journal_mode=DELETE`, `synchronous=FULL`, `foreign_keys=ON`, `busy_timeout=3000`, `page_size=4096`, `cache_size=-2048`, `mmap_size=0`, `secure_delete=OFF`, and `auto_vacuum=INCREMENTAL` set before tables are created. Keep transactions short. Do not use WAL initially.

**NEEDS EXPERIMENT:** filesystem type and sync/locking behavior at the chosen path; compare DELETE and TRUNCATE journals; confirm FULL latency and interruption durability. NORMAL may be considered only after kill/power tests. Lua opens read-only, queries, and closes immediately; it handles `BUSY` by showing stale/retry UI, not by writing.

## E. Canonical compressed self-contained HTML BLOB format

**INFERRED invariant:** the BLOB is exactly a compressed byte stream of one complete UTF-8 HTML5 document. The database columns are the envelope; there is no second custom header.

Codec IDs are stable (`1=zlib-framed DEFLATE`, `2=zstd`, `3=Brotli`). `content_format=1` means HTML5 UTF-8. `storage_version=1` defines the generator/sanitizer/image rules. Validate `uncompressed_size` before and while decoding.

```html
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Escaped title</title>
  <style>img{max-width:100%;height:auto}pre{white-space:pre-wrap}</style>
</head>
<body>
<article>
  <header><h1>Title</h1><p class="byline">Source · Author · Date</p></header>
  <!-- allowlisted semantic content -->
  <figure><img src="data:image/jpeg;base64,..." alt="..."><figcaption>...</figcaption></figure>
</article>
</body>
</html>
```

Allowed content is compact semantic markup: headings, paragraphs, `strong/em`, lists, blockquote, links, figure/caption, code/pre, simple tables, `hr`, `br`, `sup/sub`, and generated images. Remove scripts, event handlers, external CSS/fonts, forms, media, iframe/object/embed, SVG, tracking attributes, most classes/IDs, inline styles, `srcset`, and remote image references. Convert links and image URLs to absolute URLs before sanitization. Only the backend may introduce `data:` image URLs. External `http(s)` hyperlinks remain.

Base64 uses the standard padded alphabet with no line wrapping. The complete document is compressed only after all Base64 data is present.

## F. RSS/Atom and HTTP strategy

**INFERRED:** use `feed-rs`; do not implement RSS/Atom XML parsing. Preserve entry ID/GUID, alternate link, title, authors, published/updated time, `content`, and `summary`. Handle RSS 2.0 and Atom through that model; add malformed-feed fixtures before site-specific workarounds.

Content policy:

1. `feed`: use embedded content if present; fall back to summary.
2. `webpage`: always fetch/extract the page.
3. `auto` (default): use embedded HTML when it has substantial visible text/block structure and is not an obvious truncation/“read more” teaser; otherwise fetch the page. The heuristic must be fixture-tested and overridable per feed.

HTTP is synchronous and sequential. Initial limits (configuration constants, adjusted by measurements): connect 5 s; total feed request 20 s; page 30 s; image 20 s; 5 redirects; decoded feed 4 MiB; decoded page 8 MiB; encoded image 8 MiB; 8 megapixels decoded; 12 images and 24 MiB downloaded per article; generated HTML 32 MiB; compressed BLOB 16 MiB. Permit only HTTP(S), bound decompressed bodies as well as `Content-Length`, and cap every timeout by remaining run budget.

Enable gzip/DEFLATE HTTP decoding. Do not add HTTP Brotli solely for marginal transfer savings unless dependency/binary measurements justify it. Use a clear User-Agent. One failure never aborts other feeds; no immediate retry loop. A manual `--feed` may ignore schedule/backoff but still obey limits.

Store feed validators only after the feed pass has durably handled every accepted item (stored, already present, or deliberately rejected). If interrupted or a transient item failure remains, do not advance validators; a refetch is safe because the article unique key is idempotent. A 304 updates check/schedule status but writes no article data.

## G. Article extraction pipeline

**INFERRED sequence:** decode HTML to UTF-8 → remove scripts/styles/known noise and normalize lazy image sources → custom selectors if configured, otherwise `dom_smoothie::Readability` → resolve URLs → `ammonia` allowlist → DOM normalization → image stage → generated wrapper → compression.

Use feed metadata as fallback when Readability metadata is absent. A failed extraction does not create an `articles` row. Store a bounded error in `entry_failures` and retry later. After a small configurable attempt cap, mark that item skipped until its feed representation changes; never store an empty canonical article.

Start with Readability plus one optional per-feed `content_selector` and `remove_selector` supplied through CLI/database, not a site-plugin framework. Preserve useful semantics and discard presentation. Test deeply nested/malformed HTML and cap input bytes, DOM work, node count where the chosen libraries permit it, and output bytes.

## H. Kindle-specific image pipeline

**INFERRED MVP:** sequentially download → validate byte/pixel limits → decode first frame → apply EXIF orientation → resize without upscaling → convert to 8-bit grayscale → encode → Base64 → replace `img src`. If a download/decode fails, remove the image but retain useful alt text/caption.

Initial output rule: grayscale JPEG (quality around 80, benchmarked) for JPEG/WebP/GIF/photo-like inputs; grayscale PNG for PNG/alpha/line-art inputs. This is one default plus one source-based special case, not an expensive classifier. Do not persist SVG; omit or replace it with a link/alt text. Do not ship WebP output until the installed KOReader build proves reliable embedded WebP support and a size/decode win.

**NEEDS EXPERIMENT — dimensions:** query `Screen:getWidth()/getHeight()` in a probe plugin and record actual portrait/landscape reader content area with configured margins/footer. Until then no numeric resolution is authoritative.

**INFERRED parameterized resize policy:** `max_width = measured portrait content width`; `max_height = 2 × measured content height`; `max_pixels = 2 × width × height`; preserve aspect ratio and never upscale. For a long diagram, the pixel cap prevents runaway memory while width remains the primary readability constraint. There are no list thumbnails in the MVP.

**INFERRED grayscale:** store 8-bit luma. Do not pre-dither by default: it can harm screenshots/line art and compression, while KOReader/display conversion already controls e-ink output. Compare no dithering, ordered low-level dithering, and Floyd–Steinberg on the device before changing this.

## I. Compression decision

**INFERRED default: zlib-framed DEFLATE via `flate2`/`miniz_oxide`, moderate level (about 6).** It is mature, pure Rust in this configuration, simple to cross-build, streaming-friendly, and avoids zstd's C build/binary cost and Brotli's higher CPU/memory. Base64 redundancy should compress well; already-compressed image payloads limit gains from stronger codecs.

Representation metadata reserves zstd and Brotli. Benchmark all three on real article documents using DEFLATE levels 3/6, zstd 1/3, and Brotli 3/5. Compare compressed bytes, compression/decompression wall and CPU time, peak RSS, and stripped binary delta. Fast bounded decompression wins over the final few percent. Change the default only with ARM/Kindle evidence; readers must continue decoding old codec IDs.

## J. Rust/Lua interface

**INFERRED — preferred boundary after Milestone 0:** Lua performs only read queries; every mutation goes through Rust.

```text
rss-backend --db PATH doctor
rss-backend --db PATH feed add URL
rss-backend --db PATH feed remove ID
rss-backend --db PATH refresh [--budget SEC] [--feed ID] [--reason manual|wake]
rss-backend --db PATH materialize ID --cache DIR
rss-backend --db PATH mark ID read|unread
rss-backend --db PATH status [--json]
rss-backend --version
```

`materialize` prints exactly one absolute path on success. `refresh` emits no progress stdout until completion because KOReader's `dismissablePopen` blocks after output starts; structured progress/status goes to SQLite. Stderr is concise unless `--verbose`. Lua uses `util.shell_escape` on fixed arguments and numeric IDs.

If device SQLite probing fails, the fallback is Rust `list --view ... --json`; do not allow Lua writes merely to preserve direct queries. Use a disposable advisory refresh lock file to reject overlapping refresh processes; SQLite remains the authority for persistent progress.

## K. KOReader UI structure

```text
News
  Unread (count)
  All Articles
  Feeds
    Feed A (unread count)
    Feed B
  Refresh
  Status
  Settings
```

Use standard `Menu`/touch menu items for paged article lists and `KeyValuePage`, `InfoMessage`, `InputDialog`, and confirm widgets for status/settings. Each row shows unread marker, title, source, and compact date. Use keyset pages (for example 30 rows). Tap materializes and opens; hold offers mark read/unread and open external URL. No thumbnails in the MVP.

Settings remain minimal: feed add/remove/enable, refresh budget, images on/off, and retention summary. Advanced extraction/image limits remain CLI/database configuration until measurements demand UI.

## L. Materialization strategy

Rust reads the BLOB, validates codec/version and size, streams decompression into a same-directory `.tmp-PID` file, closes it, then atomically renames it to `article-{id}-s{storage_version}-{fetched_at}.html`. Reuse a valid existing file and touch its mtime. On each call purge stale `.tmp` files and enforce both file-count and byte caps by LRU mtime, never removing the file being returned.

Initial cache bound: 3 files and 32 MiB, with one oversize current article allowed. The cache directory is wholly disposable and reconstructable. No asset directory is created.

The plugin calls `ReaderUI:showReader(path, ..., after_open_callback)`. The callback invokes `mark ID read`, so an invalid/unopenable document does not become read. Mark-unread is an explicit backend call.

**NEEDS EXPERIMENT:** KOReader may create history, sidecars, or crengine cache entries for materialized HTML. These are KOReader-owned derived state, not canonical article state, but their growth and cleanup must be measured and bounded in Milestone 6.

## M. Retention/storage strategy

Run retention after refresh, never before useful work when the budget is short. Delete in small batches (for example 50 rows/transaction): oldest read articles exceeding age, per-feed cap, total cap, then soft database-size cap. Preserve unread articles where possible. A hard size ceiling may evict oldest unread only after read content is exhausted and must report that fact.

Use `compressed_size` sums for logical content and `page_count × page_size` plus `freelist_count` for physical storage. Deleting an article atomically deletes its embedded content/images. Delete matching failure rows when an article succeeds. Keep only the newest 20 `refresh_runs` and cap error text lengths.

Enable incremental auto-vacuum at database creation, but invoke `incremental_vacuum(N)` only when the freelist is materially large, in a capped batch (initially at most 128 pages) and preferably during explicit/charging maintenance. Never schedule full `VACUUM` automatically. Reuse freelist pages normally; offer manual full vacuum only with enough free space and measured need. Run `quick_check` on explicit diagnostics/upgrades, not every wake.

## N. Wake/scheduling strategy

No daemon. An external Kindle wake integration eventually invokes `refresh --budget 240 --reason wake`.

Feeds receive immutable `schedule_order` values. `app_state.scheduler_cursor` selects the first enabled due feed at/after the cursor, wrapping once. After every attempted feed, advance and commit the cursor even on failure so one slow/broken feed cannot monopolize wakes. `next_due_at` combines configured interval and backoff.

Before starting a network unit, compare monotonic remaining time with its worst allowed timeout plus a shutdown guard. Give each feed a slice (initially at most 60 s), cap new entries per pass, and stop before the global deadline. Manual refresh follows the same bounded engine.

Initial feed backoff: 15 min, 1 h, 4 h, 12 h, then 24 h cap with stable feed-derived jitter. Honor `Retry-After` for 429/503 up to 48 h. Reset on success. Do not sleep to retry within a wake run.

**NEEDS EXPERIMENT:** the correct wake hook, Wi-Fi readiness, suspend interaction, whether a process delays suspend, useful execution window, and battery impact. Milestone 7 is gated on those measurements.

## O. Crash/interruption model

- Start a `refresh_runs` row immediately. On the next invocation, mark any old unfinished run interrupted.
- Fetch/parse/extract/process outside transactions.
- Insert one complete article and clear its failure row in one short `BEGIN IMMEDIATE` transaction.
- Update feed validators, success state, next due time, and scheduler cursor only after the feed pass reaches a coherent boundary.
- Read/unread mutations are one-row transactions.
- Retention uses bounded batches.
- Unique `(feed_id,dedupe_key)` makes replay idempotent.
- A killed materialization leaves only a disposable `.tmp` file, purged next time.
- SIGKILL at any point may lose the current unit but cannot expose a half article.

Do not rely on shutdown handlers for correctness. Test repeated `kill -9` at fetch, image, compression, article insert, feed-finalization, retention, and materialization boundaries.

## P. Rust dependencies

**Provisional minimal set; every item is benchmark/build-gated:**

| Need | Choice | Rationale/status |
|---|---|---|
| HTTP/TLS | `ureq` with Rustls + bundled web PKI roots, gzip only | **INFERRED:** mature synchronous API; smaller conceptual/runtime model than Feedbook's Tokio/wreq/BoringSSL stack. Cross-build, cert time, binary size need testing. |
| Feeds | `feed-rs` | **VERIFIED in Feedbook; INFERRED reuse:** robust RSS/Atom model. |
| URL | `url` | **VERIFIED in Feedbook.** |
| Extraction | `dom_smoothie` | **VERIFIED in Feedbook; INFERRED reuse.** |
| Sanitization | `ammonia` | **VERIFIED in Feedbook; INFERRED reuse.** Measure duplicate html5ever versions/binary cost before replacing with a same-DOM allowlist. |
| SQLite | `rusqlite` + `bundled` | **VERIFIED in Feedbook; INFERRED reuse:** patched, predictable SQLite independent of glibc 2.20; requires ARM musl C toolchain. |
| Images | `image`, default features off, JPEG/PNG/GIF/WebP decode | **VERIFIED in Feedbook; INFERRED reuse.** Avoid `imagequant`, `oxipng`, Rayon. |
| Base64 | `base64` | **INFERRED:** small, maintained, streaming API. |
| Article compression | `flate2` with Rust backend | **INFERRED default.** zstd/Brotli only in benchmark features initially. |
| Dates | `chrono` (already from feed-rs) | Avoid adding a second date stack. |
| Errors | `thiserror` | **VERIFIED in Feedbook; small runtime cost.** |
| CLI | `lexopt` or a small manual parser | **INFERRED:** avoid Clap's size for seven fixed commands; choose after maintainability/size comparison. |
| Hash | `sha2` | Stable fallback dedupe hash; not for security credentials. |

Avoid Tokio/futures, Clap derive by default, serde/TOML app configuration, `dirs`, `sysinfo`, `rbook`, font/cover crates, optimization thread pools, native OpenSSL/BoringSSL, and a logging framework. Use SQLite settings and a tiny stderr logger.

Run `cargo tree -e features`, license audit, duplicate-version audit, ARM build, stripped-size comparison, and peak-RSS benchmarks before freezing dependencies.

## Q. Repository structure

```text
/
├── Cargo.toml                 # workspace
├── backend/
│   ├── Cargo.toml
│   ├── migrations/
│   └── src/
│       ├── main.rs            # thin CLI
│       ├── db.rs
│       ├── feed.rs
│       ├── http.rs
│       ├── extract.rs
│       ├── sanitize.rs
│       ├── images.rs
│       ├── article.rs         # HTML + compression representation
│       ├── refresh.rs
│       ├── schedule.rs
│       ├── retention.rs
│       └── materialize.rs
├── koreader-plugin/
│   ├── _meta.lua
│   ├── main.lua
│   ├── db.lua
│   ├── views.lua
│   └── process.lua
├── tests/
│   ├── fixtures/{feeds,html,images}/
│   └── corpus/manifest.*
├── scripts/                   # only scripts that shorten real workflows
├── Makefile                   # thin discoverable target wrapper
└── PLAN.md
```

Package deployment as `rssreader.koplugin/` with the compiled binary under `bin/`; keep the database outside the plugin directory so plugin upgrades cannot delete it.

## R. Build/test/deploy workflow

Targets:

- `make test`: format/check/lint/native unit and fixture integration tests.
- `make build`: native release/debug as appropriate.
- `make build-arm`: `cross build --release --target armv7-unknown-linux-musleabihf`.
- `make test-arm`: run static CLI/SQLite/materialization fixture smoke tests with `qemu-arm -cpu cortex-a9`.
- `make deploy-backend`, `deploy-plugin`, `deploy`: stage only this plugin, verify hashes, then atomically replace its directory while preserving the external database.
- `make kindle-test`: `doctor`, version, SQLite smoke, fixture materialization, and read-only inventory.
- `make logs`: retrieve only the capped last-run/debug output.

**VERIFIED:** QEMU supports Cortex-A9 locally. Static artifacts must pass `file`/`readelf` checks for ARM EABI hard-float, no dynamic interpreter/`NEEDED` libraries, then run `--version` and `doctor` under QEMU.

**INFERRED release baseline:** `opt-level="s"`, LTO, one codegen unit, `panic="abort"`, strip. Compare `s`, `z`, and `3` for size and Kindle runtime. Use Cortex-A9/NEON/VFPv3 flags only after the device CPU probe confirms the supplied hardware description; Feedbook uses the same flags, but that is not proof for this unit.

Deployment must first probe actual paths. Copy to a `.new` directory, validate binary execution and plugin contents, rename only `rssreader.koplugin`, and restart KOReader. Never overwrite bundled or unrelated plugins. Use SSH configuration (`ssh kindle`, `scp ... kindle:...`) rather than hard-coded host/port.

## S. Experiments required on the actual Kindle

Milestone 0 must record results, KOReader version, firmware, and commands:

1. Hardware: `/proc/device-tree/model`, CPU features, RAM, storage/free space, mount/filesystem/options, framebuffer resolution, orientation, kernel.
2. KOReader probe plugin: `Screen:getWidth()/getHeight()`, reader content area, `DataStorage` paths, plugin path/lifecycle, installed API signatures.
3. Backend: static ARM binary starts, TLS request works with correct clock/cert roots, exit codes/stdout/stderr are usable.
4. SQLite: `lua-ljsqlite3` loads; report `sqlite_version()` and compile options; read a Rust-created database; exercise concurrent short Lua reads and Rust writes.
5. Journals: DELETE vs TRUNCATE latency, lock behavior and `kill -9` recovery on the actual mount; verify `FULL` sync cost.
6. Process UI: `NetworkMgr:runWhenOnline`, escaped backend invocation, responsive/dismissable status UI, behavior if the dialog is dismissed.
7. Rendering fixture: programmatically open one HTML file with semantic tags and embedded Base64 grayscale JPEG and PNG. Confirm offline display, links, pagination, code/pre, tables, captions, rotation, and large/long images.
8. Codec matrix: embedded JPEG/PNG, and WebP only as an optional probe; inspect actual e-ink output.
9. KOReader artifacts: history, sidecars, crengine cache, file-open lifetime, and safe cache eviction.
10. Suspend/wake: Wi-Fi ready time, process during suspend, whether it delays suspend, safe budget, and wake hook.

If Base64 images fail in the installed reader, stop before Milestone 3 and revisit the rendering boundary. Do not silently create a permanent asset hierarchy.

## T. Benchmarks to collect

Create a redistributable/public-domain or synthetic, immutable corpus:

1. mostly text (about 5k words, no images);
2. photo article (4–6 photographs);
3. many-image article (at least 20 inputs to exercise caps);
4. diagrams/screenshots/line art with small text and transparency;
5. large article (about 30k words plus mixed images).

Also retain RSS 2.0, Atom, malformed feed, embedded-full-content, summary-only, duplicate GUID/URL, redirect, 304, oversized, timeout, and partial-failure fixtures. Feedbook's unlicensed fixtures/code must not be copied.

For each article record: original page bytes; extracted semantic HTML bytes; original image count/bytes/pixels; optimized image count/bytes; Base64 HTML bytes; DEFLATE/zstd/Brotli BLOB bytes; extraction, image, compression and decompression times; peak RSS; and failure/skip reason. Record stripped binary size per feature set.

On Kindle collect p50/p95 materialization/open time, feed update time, CPU time, peak RSS, database/page/freelist size at 100/1,000/10,000 synthetic articles, retention/vacuum write cost, Wi-Fi-on duration, total wake duration, and repeated A/B battery drain under controlled conditions. QEMU results are correctness/smoke evidence, not performance or compatibility proof.

## U. Risks and open questions

Priority order:

1. **NEEDS EXPERIMENT — blocker:** installed crengine support for Base64 data-URI JPEG/PNG in local HTML.
2. **NEEDS EXPERIMENT — blocker:** actual Lua SQLite availability/version and safe locking on the Kindle user-storage filesystem.
3. **NEEDS EXPERIMENT — blocker:** static musl ARM binary, TLS roots/device clock, and kernel 4.1 compatibility.
4. **NEEDS EXPERIMENT:** responsive KOReader subprocess behavior; dismissal may leave a bounded backend running.
5. **NEEDS EXPERIMENT:** actual model, framebuffer, useful viewport, RAM and storage; no numeric image dimensions are yet valid.
6. **INFERRED:** `dom_smoothie` + `ammonia` may duplicate HTML parser versions and enlarge the binary/RAM. Measure before replacing proven sanitization.
7. **INFERRED:** image decode plus Base64 plus compression may create peak-memory spikes. Strict limits and sequential work mitigate this; streaming/incremental SQLite BLOB APIs are later optimizations only if measured.
8. **NEEDS EXPERIMENT:** KOReader history/sidecars may outlive the materialization cache.
9. **NEEDS EXPERIMENT:** wake integration and power savings are device/firmware specific.
10. **VERIFIED legal risk:** Feedbook has no project license; source copying is prohibited absent permission.

## V. Milestone-by-milestone implementation plan

Each milestone is a separate Code-mode handoff. Do not pull work forward merely because an API is convenient.

### Milestone 0 — integration proof and measurements

Scope: initialize repository; record dependency/license baseline; build a trivial static ARM CLI; QEMU smoke; restore SSH authentication; inventory device; create the two-file plugin skeleton; invoke the backend without freezing the UI; prove Rust-created SQLite can be queried from Lua; materialize/open a fixture HTML; prove Base64 JPEG/PNG; record viewport/paths/versions/journal test. No RSS engine.

Exit gate: every architectural boundary works on the real Kindle, or the plan is revised. Especially no Milestone 3 assumption without data-URI proof.

### Milestone 1 — SQLite plus one feed vertical slice

Scope: schema/migrations; one manually added feed; synchronous HTTP; feed-rs adapter with GUID/content/summary; dedupe; embedded-content/simple semantic wrapper; DEFLATE BLOB; article metadata queries; plugin Unread list. Images disabled; no webpage Readability yet.

Exit gate: refresh one RSS and one Atom fixture/site, restart both processes, and show stable deduplicated unread rows from SQLite.

### Milestone 2 — offline reading

Scope: webpage fetch; Readability; allowlist sanitizer; absolute links; self-contained no-image HTML; compression/decompression; atomic materialization; ReaderUI open; after-open mark-read; mark-unread; extraction failure records.

Exit gate: RSS item → canonical BLOB → network off → open normally in KOReader; crash tests expose no partial article.

### Milestone 3 — Kindle images

Scope: bounded sequential image download; input limits; orientation; viewport resize; 8-bit grayscale; JPEG/PNG output rule; Base64 replacement; full-document recompression; real e-ink comparison corpus.

Exit gate: all five corpus classes render offline with acceptable readability, measured storage/RAM, and no remote/local asset dependency.

### Milestone 4 — complete RSS UI

Scope: multiple feeds; Unread, All, per-feed lists; counts/keyset pagination; manual refresh; status; add/remove/enable and essential settings; external link action. Reuse standard KOReader widgets only.

Exit gate: the application is usable entirely through KOReader for normal reading and manual refresh.

### Milestone 5 — efficient robust updates

Scope: ETag/Last-Modified/304; redirects; HTTP compression; all time/size caps; response validation; coherent validator commits; failure/backoff; partial item retry records; per-feed isolation; bounded structured run status.

Exit gate: deterministic tests for no network, DNS/TLS/HTTP errors, malformed/oversized/slow responses and interruption; one bad feed cannot consume the run.

### Milestone 6 — bounded storage

Scope: age/per-feed/total/byte retention; unread preference; materialization cache caps; refresh-run pruning; incremental vacuum policy; 100/1k/10k datasets; KOReader sidecar/cache assessment.

Exit gate: demonstrated steady-state bounds and reclaim behavior without routine full rewrites.

### Milestone 7 — bounded wake operation

Scope: monotonic budget; persisted round-robin cursor; per-feed slice; due scheduling; stable jitter/backoff; interruption tests; firmware-specific wake hook only after experiments.

Exit gate: repeated real wake cycles make fair progress, exit within budget/guard, recover after suspend/kill, and have recorded Wi-Fi/wake/battery measurements.

### Milestone 8 — measurement-led hardware optimization

Scope: analyze collected binary/RSS/CPU/storage/battery data; compare release profiles and codecs; tune image limits/quality and SQLite maintenance. Add concurrency, zstd, same-DOM sanitization, incremental BLOB I/O, WebP output, or deduplication only when a measured bottleneck justifies the complexity.

Exit gate: publish the final target profile and regression budgets for binary size, peak RSS, average article size, materialization latency, refresh duration, wake duration, and battery impact.

## Code-mode handoff rule

Begin with Milestone 0 only. Its output must be a short evidence report updating every **NEEDS EXPERIMENT** blocker before Milestone 1 architecture is frozen. No permanent article files or image assets, no alternate datastore, no custom renderer, and no daemon are acceptable shortcuts.
