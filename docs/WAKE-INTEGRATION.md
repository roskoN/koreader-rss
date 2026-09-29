# KOReader RSS Wake Integration

The optional wake integration schedules an approximately four-hour RTC wake,
refreshes due feeds, and lets powerd suspend normally again. It does not
replace Kindle power management or require KOReader to be running.

```text
Lua plugin  -> manages the optional service and reports status
Upstart     -> starts and supervises the Rust listener
Rust        -> listens for powerd events, arms RTC, and performs bounded refresh work
SQLite      -> stores refresh history and article state
```

On the verified PW4 firmware, `lipc-wait-event com.lab126.powerd
wakeupFromSuspend` is emitted after genuine resume. `outOfScreenSaver` is not
used because it can occur without suspend. These observations are specific to
the tested Kindle and require acceptance testing on other firmware.

## Files and lifecycle

The plugin ships `wake.lua`, `resources/koreader-rss-wake.conf`, and the
supervisor template. Enabling
**Refresh after Kindle wakes** validates prerequisites, temporarily makes the
system root writable, installs the reviewed Upstart configuration, restores
read-only state, starts the service, and verifies its status. Installation is
idempotent and includes a template version marker. Disabling the option stops
the service and removes the configuration using the same read-only restoration
guard. The plugin never installs the service merely because it is present.

The Upstart job starts one long-lived Rust process. The process owns both event
listeners and is frozen with userspace during suspend:

```text
readyToSuspend 1 -> calculate (next_refresh_at - now) -> lipc-set-prop rtcWakeup N
wakeupFromSuspend -> wait 15s -> check powerd.state and deadline -> refresh if due
```

The backend and database paths are safely substituted when the service is
installed. `lipc-wait-event` is the only event wait; there is no suspended-state
polling. After wake, the process checks `powerd.state` and the persisted
absolute deadline before refreshing, then releases its temporary suspend
deferral.

## Backend wake policy

`schedule` persists an absolute `next_refresh_at` in SQLite. Each suspend
cycle calculates the remaining time rather than resetting the interval. The
`refresh --reason wake` path uses the same implementation as manual refresh, with
wake-specific safeguards:

- the refresh lock rejects overlap immediately;
- a not-yet-due wake exits cheaply;
- the default scheduled wake-check cadence is four hours; once the persisted
  application deadline is due, every enabled feed without an active failure
  backoff is checked, regardless of its normal successful-refresh interval;
- network and HTTP work are bounded by the supplied budget;
- failed feeds are skipped until their persisted retry/backoff deadline;
- successful and skipped attempts remain visible through `status`.

On a completely successful wake, the next scheduled check is four hours later.
After a wake with failures, the global wake interval is doubled (up to 24 hours),
and individual failed feeds also retain their own retry deadlines. This avoids
repeatedly requesting feeds that are failing while still checking healthy feeds
at each scheduled wake.

Typical invocation:

```sh
/mnt/us/koreader/plugins/rssreader.koplugin/bin/rss-backend \
  --db /mnt/us/koreader/data/rssreader/rss.sqlite3 \
  refresh --budget 600 --reason wake
```

Refresh metadata records trigger, duration, feeds checked, new articles,
outcome, and error/skip reason. SQLite remains authoritative; service failures
are supervised by Upstart. No application log files are written; inspect
refresh status in SQLite using the backend `status` command.

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
integration does not use `outOfScreenSaver`, poll while suspended, request raw
`/sys/class/rtc` alarms, retry continuously, or wait indefinitely for Wi-Fi.
The temporary `deferSuspend` request is released on completion; its exact
suspend behavior remains `NEEDS EXPERIMENT` on the target firmware.

## Validation status

Host validation covers Lua syntax, backend tests, template handling, locking,
staleness, and bounded refresh behavior. Device acceptance still requires
several genuine suspend/resume cycles: confirm one wake attempt per cycle,
test screensaver-only transitions and unavailable Wi-Fi, then disable the
feature and verify normal resuspend and battery behavior. Host or QEMU results
do not establish Kindle suspend, Wi-Fi, or battery behavior.
