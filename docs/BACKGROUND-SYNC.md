# Background Synchronization

## Overview

Feed refresh work remains bounded, but the optional powerd integration runs the
Rust event listener as a long-lived process. It does not poll while suspended.

The persisted scheduler stores each feed's `next_due_at` and selects enabled,
due feeds in deterministic schedule order. Each feed and its articles are
processed sequentially; refresh progress and outcomes are stored in
`refresh_runs`.

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
persisted backoff. RSS article content is committed before full-page processing
so an interrupted article retains a usable fallback.

## Current trigger state

The optional suspend/resume integration is managed by the plugin and uses an
Upstart supervisor to start one long-lived Rust listener for
`com.lab126.powerd`'s `readyToSuspend` and `wakeupFromSuspend` events. It is
disabled until the user explicitly enables it. The listener arms `rtcWakeup`
through powerd from the persisted absolute deadline and does not poll while
suspended. See
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
`lipcPropErrInvalidState`; setting it during `readyToSuspend 1` is the verified
arming path for this integration. Full repeated-cycle behavior, temporary
`deferSuspend` semantics, Wi-Fi readiness, and battery impact remain
`NEEDS EXPERIMENT`.

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
[`docs/WAKE-EXPERIMENT.md`](WAKE-EXPERIMENT.md); the production path uses
powerd's `rtcWakeup` arbitration rather than raw RTC sysfs writes.

## Wake refresh lifecycle

The installed supervisor starts the long-lived backend listener:

```text
/mnt/us/koreader/plugins/rssreader.koplugin/bin/rss-backend \
  --db /mnt/us/koreader/data/rssreader/rss.sqlite3 powerd-daemon
```

The listener arms the persisted due time during `readyToSuspend`, waits for
`wakeupFromSuspend`, settles for approximately 15 seconds, checks powerd state,
and releases temporary suspend deferral after bounded refresh work. The
one-shot wrapper remains available for manual/device diagnostics. Refresh
history and outcomes are available from SQLite via the backend `status` command.

The normal successful wake-check interval is four hours. At a scheduled wake,
all enabled feeds without a pending failure backoff are checked even if their
ordinary successful-refresh interval has not elapsed. Failed feeds respect
their persisted retry deadline. The wake refresh retains its configured
600-second work budget.

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
