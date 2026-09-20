# Manual Kindle / KOReader validation checklist and observation log

Template for the device-only experiments called out in PLAN §C, §H, §L, §N, §S
and the open items in STATUS.md. It drives the physical checks that host/QEMU
cannot establish. Fill the observation log as you go; leave evidence labels at
`NEEDS EXPERIMENT` until a real device run produces a result.

> Derived from PLAN.md (architecture + required experiments) and STATUS.md
> (completed items + current unverified behavior). Do not treat this file as a
> spec; it records what to check on the installed device, not what to build.

## Evidence-label legend

- **VERIFIED** — confirmed by direct measurement in this session on the Kindle.
- **INFERRED** — supported by host/QEMU or source, but not yet measured on-device.
- **NEEDS EXPERIMENT** — unverified on the installed Kindle/KOReader.

## 0. Pre-flight: device and version fields

Record before any probe. Every later result is meaningless without these.

| Field | Expected (reference) | Observed | Evidence |
|---|---|---|---|
| Model | `Lab126 i.MX6SLL Board` | | `/proc/device-tree/model` |
| Kernel/firmware | `4.1.15-lab126` | | `uname -a` |
| RAM | 502 MiB | `cat /proc/meminfo` | |
| User storage free | ≥5.8 GiB | `df -k` | |
| Mount point | `/mnt/us` | `mount \| grep fuse` | |
| Filesystem | `fuse.fsp` | | |
| Installed KOReader | `v2026.07.1` | | Settings → About |
| Installed plugin path | `plugins/rssreader.koplugin/` | | `ls` |
| Installed SQLite (`lua-ljsqlite3`) | version + compile opts | | §3 |

Reproduce via `make kindle-test` (sets `KOREADER_DIR`, runs the inventory
commands below) or `./scripts/kindle-logs.sh`.

## 1. KOReader plugin menu navigation

The plugin registers a single main-menu entry (per `_meta.lua`), named
**RSS Reader**. Restart KOReader
after any deploy; the menu is not visible until then.

**Path:** Main menu → *RSS Reader* → sub-items:

1. Latest articles
2. Refresh now / Refresh status
3. Wake refresh (Check status / Enable / Disable)
4. Feeds / Add feed
5. Environment and paths
6. Run backend doctor
7. Test HTTPS and certificates
8. Initialize and query SQLite
9. Open offline HTML fixture

Expected per item is recorded in §9 (feed UI) and the relevant sections below.

## 2. Host / QEMU gates (complete before any Kindle claim)

These are not device experiments but gate the device work; record pass/fail.

- `cargo fmt --all --check` → clean
- `cargo clippy --workspace --all-targets -- -D warnings` → no warnings
- `cargo test --workspace` → current workspace tests pass
- `luajit -b koreader-plugin/main.lua /tmp/rssreader-main.luac` → compiles
- `make build-arm` → `cross build --release --target armv7-unknown-linux-musleabihf --package rss-backend`
- QEMU smoke: `qemu-arm -cpu cortex-a9 <bin> --version`, `doctor`, `init-probe-db`,
  `materialize-fixture` (must emit `data:image/png;base64,` and
  `data:image/jpeg;base64,`)
- `file`/`readelf`: ARM EABI hard-float, no dynamic interpreter, no `NEEDED` libs

## 3. SQLite runtime (on device)

Driven by menu item **Initialize and query SQLite**; probe DB is
`DataStorage:getDataDir()/data/rssreader/probe.sqlite3`.

| Check | Command (backend) | Expected | Evidence |
|---|---|---|---|
| Lua SQLite loads | menu: Initialize and query SQLite | `SQLite probe failed` not shown | |
| `sqlite_version()` | `queryProbeDatabase` | non-empty version string | |
| Compile options | `queryProbeDatabase` | compile flags printed | |
| `PRAGMA journal_mode` | `queryProbeDatabase` | `DELETE` (PLAN §D default) | |
| Short read + write | re-run after creating a DB via Rust | read returns, write returns | |
| Concurrent short reads vs Rust write | two readers while a `refresh` writes | no `BUSY` crash; stale data shown | |

If the Lua probe fails, fall back to `rss-backend --db PATH list --view ... --json`
(do not introduce Lua writes just to preserve direct queries).

## 4. Base64 rendering (on device)

Driven by menu item **Open offline HTML fixture** (materializes
`cache/rssreader/integration-probe.html` via
`materialize-fixture --out`), then `ReaderUI:showReader`.

| Check | Expected | Evidence |
|---|---|---|
| Offline display of HTML | renders with no network | |
| Embedded grayscale JPEG data URI | renders, no red X | |
| Embedded grayscale PNG data URI | renders, no red X | |
| Links, code/pre, simple tables, caption | render correctly | |
| Rotation / orientation | matches file EXIF | |
| Large/long image | readable, no crash | |
| WebP (optional probe only) | render or fail | |

If Base64 images fail, per PLAN §L this **stops before Milestone 3** — do not
silently create a permanent asset hierarchy.

## 5. Viewport (on device)

Do not record a numeric resolution as authoritative until measured.

| Check | Command | Expected | Evidence |
|---|---|---|---|
| `Screen:getWidth()/getHeight()` | minimal probe plugin | portrait/landscape area | |
| Reader content area | probe inside reader | minus margins/footer | |
| Framebuffer modes | `fb0/virtual_size`, `/modes` | `1072×1448` / `1448×1072` | |
| `Device.screen` values | menu: Environment and paths | matches probe | |

Use the measured portrait content width/height to derive the inferred resize
policy (§H): `max_width`, `max_height = 2×`, `max_pixels = 2×` (never upscale).

## 6. Locking / journal / interruption (on device)

| Check | Method | Expected | Evidence |
|---|---|---|---|
| Journal mode behavior | §3 `PRAGMA journal_mode` | `DELETE`, no WAL side files | |
| `kill -9` during write | kill backend mid-`refresh`/`materialize` | no half article exposed | |
| Recovery after kill | re-run operation | resumes from persistent state | |
| Concurrent read while write open | §3 concurrent reads | read returns, no crash | |
| `synchronous=FULL` cost | time a small refresh | acceptable latency | |
| Interrupt durability | kill mid-commit, inspect DB | article-all-or-nothing | |
| Advisory refresh lock | overlap two `refresh` runs | second rejected | |

## 7. Cache artifacts (on device)

| Check | Expected | Evidence |
|---|---|---|
| Disposed cache bound | ≤3 files / 32 MiB, requested article retained | `ls -la cache/rssreader/` |
| Stale `.tmp-PID` purge | no leftover temp after operation | |
| Oversize current article allowed | one large file may exceed bound | |
| Reconstructability | BLOB re-materializes after cache loss | re-materialize ID |
| KOReader history/sidecars/crengine cache | measure growth & cleanup (PLAN §L, §U#8) | `df`, file listing |

## 8. Feed UI (on device)

| Check | Path | Expected | Evidence |
|---|---|---|---|
| Initialize SQLite | Main → RSS Reader → Initialize and query SQLite | probe results shown | §3 |
| Feeds list | → Feeds | parses `feed list` tab-separated | |
| Add feed | → Add feed | prompts URL, runs `feed add URL` | |
| Remove feed | hold a feed row | confirm, runs `feed remove ID` | |
| Latest article list | → Latest articles | reads newest `articles` join, ≤100 rows, count in title | |
| Open article | tap latest row | materialize + `showReader`; no read-state write | §4 |
| Refresh | Main menu / `refresh` | bounded, structured status in SQLite | |
| Status | → Status | shows run counters/outcome | |
| External link | hold article | opens URL in stock reader | |

## 9. Wake / power behavior (Milestone 7 gate)

All `NEEDS EXPERIMENT` per PLAN §N and §U#9.

| Check | Method | Expected | Evidence |
|---|---|---|---|
| Wi-Fi readiness time | trigger refresh on wake | time to first fetch | |
| Process delays suspend? | run bounded `refresh --budget 240 --reason wake`, then suspend | measured awake duration, battery impact | |
| Correct wake hook | experiment only after §6/§8 done | bounded progress, exits within budget/shutdown guard | |
| Resume after suspend/kill | re-run after suspend | makes fair progress from persisted cursor | |
| Backoff timing | 15m/1h/4h/12h/24h + jitter | matches schedule after failures | |

## 10. Observation log template

Copy this block per experiment. Keep it to one page per device session.

```text
Experiment: <name>                       Date: <YYYY-MM-DD>
Device:   model=___ kernel=___ RAM=___ storage_free=___ mount=___ fs=___
KOReader: v___    Plugin path: ___.koplugin/   SQLite: v___ opts=___
Operator: <name>        Firmware patch: ___

Step: <menu item or CLI>
Command: <exact command or menu path>
Expected: <what a pass looks like>
Observed: <what happened>
Evidence: VERIFIED | INFERRED | NEEDS EXPERIMENT

Notes / anomalies:
Blocker? If yes, what external action unblocks it:
```

## 11. Backend CLI reference (used above)

All commands take `--db PATH` (data DB) and, where noted, `--cache DIR`.

- `rss-backend --db PATH doctor` → §5
- `rss-backend --db PATH http-probe --url URL` → §5 (HTTPS/certs)
- `rss-backend --db PATH status [--json]` → §9 Status
- `rss-backend --db PATH feed add URL` → §9 Add feed
- `rss-backend --db PATH feed list` → §9 Feeds
- `rss-backend --db PATH feed remove ID` → §9 Remove feed
- `rss-backend --db PATH refresh [--budget SEC] [--feed ID] [--reason manual|wake]` → §9 Refresh
- `rss-backend --db PATH materialize ID --cache DIR` → §4 (prints one absolute path)
- `rss-backend --db PATH materialize-fixture --out DIR` → §4
- `rss-backend --db PATH init-probe-db --db PATH` → §3

`materialize` must print exactly one absolute path on success; `refresh` emits no
progress stdout until completion (structured status goes to SQLite).

## 12. Uncertainty carried forward from STATUS.md

## 13. Automated validation

Run `make validate` before a device session. It exercises a fixture database,
50 short-timeout materialization interruption boundaries, temporary-file
cleanup, cache reconstruction, and the database probe. Run
`make validate-device` after deploying the backend to run the same fixture,
materialization timing, database/cache probe, and concurrent read checks over
SSH. These checks do not establish KOReader rendering, plugin menu behavior,
or suspend/battery behavior; retain those observations in the log above.

- Hardware SQLite/data-URI rendering, filesystem locking/journal interruption,
  and wake/suspend behavior remain **NEEDS EXPERIMENT** (§1, §6, §10).
- Wake hook, Wi-Fi readiness, suspend interaction, execution window, battery
  impact remain **NEEDS EXPERIMENT** (§10).

## 14. Unattended refresh job

The deployed plugin contains `refresh-job.sh`. It is a one-shot, bounded
entrypoint for a verified Kindle wake/powerd hook. It waits up to one minute
for HTTPS readiness, invokes the backend with `--reason wake --budget 60`,
rotates a 256 KiB log, and exits without changing power-management state. It
does not install a daemon or schedule itself. Before wiring it to a firmware
wake mechanism, verify that the hook wakes Wi-Fi, permits the bounded process
to finish, and restores normal suspend behavior.
