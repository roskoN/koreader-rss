# KOReader RSS Wake Integration

The optional wake integration refreshes feeds after the Kindle completes a
real suspend/resume cycle. It does not schedule a wake, replace Kindle power
management, or require KOReader to be running.

```text
Lua plugin  -> manages the optional service and reports status
Upstart     -> waits for powerd's genuine wake event
Rust        -> applies refresh policy and performs bounded network/database work
SQLite      -> stores refresh history and article state
```

On the verified PW4 firmware, `lipc-wait-event com.lab126.powerd
wakeupFromSuspend` is emitted after genuine resume. `outOfScreenSaver` is not
used because it can occur without suspend. These observations are specific to
the tested Kindle and require acceptance testing on other firmware.

## Files and lifecycle

The plugin ships `wake.lua` and `resources/koreader-rss-wake.conf`. Enabling
**Refresh after Kindle wakes** validates prerequisites, temporarily makes the
system root writable, installs the reviewed Upstart configuration, restores
read-only state, starts the service, and verifies its status. Installation is
idempotent and includes a template version marker. Disabling the option stops
the service and removes the configuration using the same read-only restoration
guard. The plugin never installs the service merely because it is present.

The Upstart job is intentionally policy-free:

```text
wait for wakeupFromSuspend
run rss-backend refresh --budget 30 --reason wake
exit
Upstart respawns and waits for the next wake
```

The backend and database paths are safely substituted when the service is
installed. The backend remains a short-lived process; no custom daemon or
power-state polling is introduced.

## Backend wake policy

`refresh --reason wake` uses the same implementation as manual refresh, with
wake-specific safeguards:

- the refresh lock rejects overlap immediately;
- a recent successful refresh causes an immediate, recorded skip;
- network and HTTP work are bounded by the supplied budget;
- feed failures use persisted retry/backoff state;
- successful and skipped attempts remain visible through `status`.

Typical invocation:

```sh
/mnt/us/koreader/plugins/rssreader.koplugin/bin/rss-backend \
  --db /mnt/us/koreader/data/rssreader/rss.sqlite3 \
  refresh --budget 30 --reason wake
```

Refresh metadata records trigger, duration, feeds checked, new articles,
outcome, and error/skip reason. SQLite remains authoritative; no permanent
wake log is required for normal operation.

## Diagnostics and safety

The plugin's **Wake refresh** menu reports support, installation, running
state, template version, and the latest backend refresh status. Equivalent
device checks are:

```sh
status koreader-rss-wake
/mnt/us/koreader/plugins/rssreader.koplugin/bin/rss-backend \
  --db /mnt/us/koreader/data/rssreader/rss.sqlite3 status
```

Unsupported devices leave the system unchanged. Installation errors attempt to
restore the root filesystem to read-only before reporting failure. The
integration does not use `outOfScreenSaver`, poll power state, request an
opportunistic wake, retry continuously, or wait indefinitely for Wi-Fi. RTC
alarms are deliberately a separate future feature.

## Validation status

Host validation covers Lua syntax, backend tests, template handling, locking,
staleness, and bounded refresh behavior. Device acceptance still requires
several genuine suspend/resume cycles: confirm one wake attempt per cycle,
test screensaver-only transitions and unavailable Wi-Fi, then disable the
feature and verify normal resuspend and battery behavior. Host or QEMU results
do not establish Kindle suspend, Wi-Fi, or battery behavior.
