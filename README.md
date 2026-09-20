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
- Provides latest-article, per-feed, refresh, status, and feed-management views
  in KOReader.
- Retains the newest articles by age and configured limits without tracking
  read/unread state.
- Supports OPML import from the plugin's `feeds` directory.
- Keeps a bounded disposable materialization cache.

## KOReader options

Open **RSS Reader** from the KOReader main menu.

- **Latest articles** — list stored articles newest first with paging.
- **Feeds** — view feeds, inspect errors, open feed articles, enable/disable,
  or remove a feed.
- **Add feed** — enter and validate an RSS/Atom URL before saving it.
- **Remove all articles** — permanently delete stored article content after
  confirmation.
- **Remove all feeds** — permanently delete feeds and their dependent articles
  after confirmation.
- **Refresh now** — process all due feeds without an overall time limit.
- **Refresh status** — display the latest refresh outcome and counters.
- **Wake refresh** — open explicit **Check status**, **Enable**, and **Disable**
  actions for refresh after a genuine Kindle suspend/resume cycle.
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

Manual refresh is available from the **Refresh now** menu item. It processes
all due feeds and entries until completion; individual network requests still
use their normal connection/read timeouts. The backend also has a one-shot
unattended wrapper:

```text
/mnt/us/koreader/plugins/rssreader.koplugin/refresh-job.sh
```

It waits for HTTPS readiness, runs a short `--reason wake` refresh, writes a
bounded rotating log, and exits. The optional powerd/Upstart integration
waits for the genuine `wakeupFromSuspend` event and runs the same bounded
backend command. It is never installed silently. See
[docs/WAKE-INTEGRATION.md](docs/WAKE-INTEGRATION.md).

### Wake refresh menu

The **Wake refresh** menu provides three explicit actions:

- **Check status** — report device support, service state, and configuration
  version.
- **Enable** — install the versioned Upstart listener and start it after safely
  restoring the system root to read-only.
- **Disable** — stop the listener and remove its configuration.

The listener waits for `com.lab126.powerd`'s `wakeupFromSuspend` event, then
runs a short `rss-backend refresh --reason wake` command. The backend enforces
the refresh lock, minimum interval, retry policy, and time budget. KOReader
does not need to be running when the Kindle wakes. The feature does not create
RTC alarms, poll power state, or use `outOfScreenSaver` as a trigger.

## Installation for users

Download the ZIP archive from the repository's [GitHub Releases](https://github.com/roskoN/koreader-rss/releases)
page. The current public release is `v0.0.2`. The package is intended for a
jailbroken **Kindle Paperwhite 4 (10th generation)** running KOReader.

1. Install KOReader on the jailbroken Kindle and start it once.
2. Download `rssreader-0.0.2.zip` (or a newer release) on your computer.
3. Extract the archive. It contains a directory named `rssreader.koplugin`.
4. Connect the Kindle over USB and copy that complete directory to:

   ```text
   /mnt/us/koreader/plugins/rssreader.koplugin/
   ```

   Merge only when upgrading an existing installation; do not rename the
   directory or copy only the Lua files. The archive contains the ARMv7
   backend required by the Kindle.
5. Safely eject the Kindle, restart KOReader, and open **RSS Reader** from the
   main menu.

The package has been exercised on a Kindle Paperwhite 4 with kernel
`4.1.15-lab126` and KOReader `v2026.07.1`. Feed fetching, backend execution,
SQLite probes, fixture materialization, and package deployment were verified on
that device. KOReader visual rendering, suspend/resume wake behavior, Wi-Fi
readiness, and battery impact still require device-specific validation; see
[docs/MANUAL-VALIDATION.md](docs/MANUAL-VALIDATION.md).

To remove the plugin, exit KOReader and delete the
`/mnt/us/koreader/plugins/rssreader.koplugin/` directory. This does not modify
the stock Kindle UI or its unrelated data.

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
make deploy KOREADER_DIR=/mnt/us/koreader
```

`make package` creates a complete plugin bundle containing the Lua files,
backend, `feeds/` OPML import directory, and wake integration resources.
Restart KOReader after deploying plugin changes.

### Live feed smoke tests

`make test-feeds` performs bounded live `feed check` requests against:

- `https://www.theverge.com/rss/index.xml`
- `https://feeds.arstechnica.com/arstechnica/index`

The test verifies that both feeds are reachable, parse successfully, and
contain entries. It requires network access and is separate from the
deterministic workspace test suite.

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
- 512 MiB database target.
- Three materialized HTML files / 32 MiB cache.

Pruning always removes the oldest articles first, regardless of whether they
have been opened. This keeps the newest available articles within the feed,
total-count, age, and database-size bounds. Existing databases migrate away
from the former read/unread columns when the backend first opens them.

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

## License

This project is licensed under the [MIT License](LICENSE).
