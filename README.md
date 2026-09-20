# RSS Reader for Kindle

An offline-first RSS/Atom reader for a jailbroken Kindle Paperwhite 4. The
application keeps the normal Kindle experience intact while providing a small
KOReader interface for feeds and locally stored articles.

## What it does

- Imports RSS and Atom feeds.
- Validates feeds before adding them.
- Stores feed titles, validators, scheduling state, and failure information.
- Fetches the full article page for each RSS entry when available.
- Uses Readability extraction and allowlist sanitization.
- Falls back to usable RSS content when the article page cannot be fetched.
- Downloads, orients, converts, resizes, and embeds article images.
- Compresses complete article HTML before storing it in SQLite.
- Provides unread, all-article, per-feed, refresh, status, and feed-management
  views in KOReader.
- Supports OPML import from the plugin's `feeds` directory.
- Keeps a bounded disposable materialization cache.

## KOReader options

Open **RSS Reader** from the KOReader main menu.

- **Unread** — list unread articles.
- **All articles** — list stored articles with paging.
- **Feeds** — view feeds, inspect errors, open feed articles, enable/disable,
  or remove a feed.
- **Add feed** — enter and validate an RSS/Atom URL before saving it.
- **Remove all articles** — permanently delete stored article content after
  confirmation.
- **Remove all feeds** — permanently delete feeds and their dependent articles
  after confirmation.
- **Refresh now** — run a bounded manual refresh.
- **Refresh status** — display the latest refresh outcome and counters.
- **Wake refresh** — inspect support/status and optionally enable refresh after
  a genuine Kindle suspend/resume cycle.
- **Environment and paths** — show application paths and device information.
- **Run backend doctor** — run critical SQLite/runtime checks.
- **Test HTTPS and certificates** — verify network access.
- **Initialize and query SQLite** — create/query the probe database.
- **Open offline HTML fixture** — open a known local article fixture.

Long-pressing an article opens its original URL through the device link
handler. Long-pressing a feed toggles its enabled state.

## OPML import

Place one or more OPML files directly on the Kindle in:

```text
/mnt/us/koreader/plugins/rssreader.koplugin/feeds/
```

KOReader imports every `xmlUrl` when the plugin starts. Successfully processed
OPML files are deleted afterward; files that fail to read or process remain for
retry. Imports are idempotent.

## Refresh behavior

Manual refresh is available from the **Refresh now** menu item. The backend
also has a one-shot unattended wrapper:

```text
/mnt/us/koreader/plugins/rssreader.koplugin/refresh-job.sh
```

It waits for HTTPS readiness, runs a short `--reason wake` refresh, writes a
bounded rotating log, and exits. The optional powerd/Upstart integration
waits for the genuine `wakeupFromSuspend` event and runs the same bounded
backend command. It is never installed silently. See
[docs/WAKE-INTEGRATION.md](docs/WAKE-INTEGRATION.md).

## Installation and development

The repository provides Make targets for the verified Kindle target:

```text
make test                 # format, Clippy, and workspace tests
make test-feeds           # live RSS checks for The Verge and Ars Technica
make validate             # host interruption/cache validation
make benchmark            # materialization timing and RSS measurement
make test-arm             # ARM cross-build and QEMU smoke tests
make validate-device      # SSH device diagnostics
make deploy-backend KOREADER_DIR=/mnt/us/koreader
make deploy-plugin KOREADER_DIR=/mnt/us/koreader
```

Restart KOReader after deploying plugin changes.

## Technical setup

### Components

```text
KOReader Lua plugin
        |
        | bounded subprocess commands
        v
Rust rss-backend -------------- HTTPS feeds/articles/images
        |
        v
SQLite authoritative store
        |
        +-- compressed article HTML BLOBs
        +-- feed/scheduler/refresh state
        +-- read/unread state
```

The Lua layer handles menus, user actions, subprocess invocation, and opening
materialized documents. Rust handles network access, parsing, extraction,
sanitization, image processing, compression, scheduling, and SQLite writes.

### Persistent storage

SQLite is authoritative. Article HTML and embedded image data are stored in a
compressed BLOB. Materialized HTML files are disposable and reconstructable.

Default retention settings are:

- 500 articles per feed.
- 5,000 articles total.
- 90-day retention.
- 256 MiB database target.
- Three materialized HTML files / 32 MiB cache.

Pruning prefers read articles and performs bounded incremental freelist vacuum.
Unread content can temporarily exceed limits when it is the only content left.

### Scheduling

Feeds have immutable scheduling order, persisted due times, backoff, and
stable jitter. Due feeds are selected fairly using a persisted cursor. Refresh
runs record progress and outcomes, and an overlap lock prevents concurrent
refreshes.

### Article processing

For entries with URLs, the backend prefers the linked full article page. It
uses Readability extraction followed by an HTML allowlist sanitizer. Anchor
elements and `href` attributes are removed while visible link text is retained.
If the page fails, usable RSS content is preserved as a fallback.

Images are bounded, EXIF-oriented, converted to grayscale, resized, encoded,
embedded as data URLs, and compressed with the complete article document.

### Evidence and limitations

Host, QEMU, and selected Kindle backend checks are automated. KOReader visual
rendering, actual Lua SQLite runtime behavior, suspend/wake behavior, Wi-Fi
readiness, sidecars, and battery impact require real-device validation.

See:

- [PLAN.md](PLAN.md) — roadmap and acceptance gates.
- [STATUS.md](STATUS.md) — current implementation state and evidence.
- [docs/BACKGROUND-SYNC.md](docs/BACKGROUND-SYNC.md) — unattended refresh.
- [docs/WAKE-INTEGRATION.md](docs/WAKE-INTEGRATION.md) — suspend/resume wake
  integration.
- [docs/MANUAL-VALIDATION.md](docs/MANUAL-VALIDATION.md) — device validation checklist.
