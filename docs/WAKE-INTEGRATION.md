# KOReader RSS Wake Integration

The plugin schedules refreshes through KOReader's `Device.wakeup_mgr`. KOReader
owns Kindle RTC/powerd integration, including programming the alarm at
`ReadyToSuspend` and validating scheduled wakes after resume. Rust owns the
persisted deadline and performs feed refresh work.

```text
Lua plugin  -> reads backend deadline, schedules/removes a WakeupMgr task
KOReader    -> arms and validates Kindle wakeups through its power lifecycle
Rust        -> persists deadlines and performs bounded refresh work
SQLite      -> stores refresh history and article state
```

## Lifecycle

The plugin is not document-only, so KOReader loads it in both file-manager and
reader sessions; either running mode can register the same persisted schedule.
At plugin initialization and after refresh completion, Lua runs the backend's
`schedule` command, parses `next_refresh_at`, removes any previous task by its
callback reference, and registers one replacement task with
`Device.wakeup_mgr:addTask`. Past deadlines are clamped to a one-second delay.
The backend's SQLite state retains feed URLs, refresh interval, and last
successful refresh. The WakeupMgr callback queues a fallback one-shot task
before attempting network work, so a failed connection cannot consume the only
scheduled task. If the radio is offline, Lua asks NetworkMgr to enable Wi-Fi and
starts the bounded refresh only from its asynchronous connected callback; there
is no blocking Wi-Fi wait loop. Normal user resumes do not trigger refresh.
After a completed backend run, Lua rereads persisted scheduling state and
replaces the fallback task. Failed backend runs use a 60-second minimum retry.

The integration does not install or listen for powerd events itself:

```text
Lua addTask -> KOReader mockrtc retains desired epoch
ReadyToSuspend -> KOReader/powerd programs rtcWakeup
scheduled resume -> KOReader validates WakeupMgr proximity -> callback -> Rust refresh
```

Rust does not write RTC state, call LIPC, start an event listener, or request a
forced suspend. After the backend exits, normal KOReader/powerd inactivity and
autosuspend behavior applies; its exact behavior during background refresh is
`NEEDS EXPERIMENT` on Kindle.

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
outcome, and error/skip reason. SQLite remains authoritative. No application
log files are written; inspect refresh status in SQLite using the backend
`status` command.

## Diagnostics and safety

Inspect persisted scheduler and refresh state with:

```sh
/mnt/us/koreader/plugins/rssreader.koplugin/bin/rss-backend \
  --db /mnt/us/koreader/data/rssreader/rss.sqlite3 status
```

The integration does not use `outOfScreenSaver`, poll while suspended, request
raw `/sys/class/rtc` alarms, manipulate `rtcWakeup`, or wait indefinitely for
Wi-Fi. The previous optional `koreader-rss-wake` Upstart service is retired;
remove any older installation before using this integration.

## Validation status

Host validation covers Lua syntax, backend tests, template handling, locking,
staleness, and bounded refresh behavior. Device acceptance still requires
several genuine suspend/resume cycles: confirm one wake attempt per cycle,
test screensaver-only transitions and unavailable Wi-Fi, then disable the
feature and verify normal resuspend and battery behavior. Host or QEMU results
do not establish Kindle suspend, Wi-Fi, or battery behavior.
