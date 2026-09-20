# Background Synchronization

## Overview

Background synchronization is designed as a short-lived, bounded backend
operation. The project does not run a permanent RSS daemon on the Kindle.

The persisted scheduler stores each feed's `next_due_at` and selects enabled,
due feeds fairly using `app_state.scheduler_cursor`. Refresh progress is stored
in `refresh_runs`, so a later invocation can continue after interruption.

## Refresh entrypoint

The deployed plugin contains:

```text
/mnt/us/koreader/plugins/rssreader.koplugin/refresh-job.sh
```

This is a one-shot unattended-refresh wrapper. It:

1. Creates the application data, cache, and log directories.
2. Rotates the refresh log when it exceeds 256 KiB.
3. Waits up to 60 seconds for HTTPS readiness.
4. Runs the bounded backend command:

   ```text
   rss-backend --db .../rss.sqlite3 refresh --reason wake --budget 60
   ```

5. Records success, failure, or network-unavailable state in the log.
6. Exits without changing keep-awake or suspend settings.

The backend refresh lock prevents overlapping invocations. Feed failures use
persisted backoff, and successful work advances the fair scheduler cursor.

## Current trigger state

The optional suspend/resume integration is managed by the plugin and uses an
Upstart listener for `com.lab126.powerd`'s `wakeupFromSuspend` event. It is
disabled until the user explicitly enables it. The service does not schedule
RTC alarms, poll power state, or run when the feature is disabled. See
[`WAKE-INTEGRATION.md`](WAKE-INTEGRATION.md) for the complete lifecycle.

## Investigated Kindle interfaces

On the PW4 test device, the following interfaces exist:

- `com.lab126.powerd` LIPC publisher.
- Write-only `rtcWakeup` integer property.
- Write-only `rtcWakeup2` string property.
- `wakeUp` powerd action.
- `/usr/sbin/rtcwake`.
- `/usr/sbin/crond`.

An attempt to set `rtcWakeup` while the device was active returned
`lipcPropErrInvalidState`. This establishes that the property is state-sensitive
but does not verify the required value format or suspend/wake behavior.

## Required device acceptance experiment

Before treating the integration as verified:

1. Record the current powerd state, battery level, and Wi-Fi state.
2. Enable **Refresh after Kindle wakes** from the plugin.
3. Verify `status koreader-rss-wake` reports a running listener.
4. Suspend and wake the Kindle normally for several cycles.
5. Confirm one `reason=wake` attempt per genuine resume and measure Wi-Fi
   readiness and total awake time.
6. Test screensaver-only transitions and unavailable Wi-Fi.
7. Disable the feature and verify the service is stopped and removed.
8. Verify normal suspend behavior and battery impact.

The experiment must be repeated across several cycles before the behavior is
classified as `VERIFIED`. Host and QEMU results cannot establish Kindle power,
Wi-Fi, or battery behavior.

The exact command-by-command RTC investigation remains in
[`docs/WAKE-EXPERIMENT.md`](WAKE-EXPERIMENT.md), but RTC alarms are not needed
for the production suspend/resume integration.

## Intended final integration

Once a wake mechanism is verified, it should invoke only the one-shot wrapper:

```text
/mnt/us/koreader/plugins/rssreader.koplugin/refresh-job.sh
```

The schedule should be no more frequent than the configured feed interval,
respect the persisted due times, and avoid leaving a keep-awake request or
background process behind. If reliable Kindle wake scheduling cannot be
established, the fallback is to invoke this wrapper whenever the device is
reachable over SSH or when KOReader resumes.

## Validation commands

Host and QEMU checks:

```text
make validate
make validate-device
make test-arm
```

The device diagnostics verify backend execution, SQLite/cache probes,
materialization timing, and concurrent reads. They do not verify wake,
rendering, suspend, or battery behavior.
