# KOReader RSS Wake Integration Plan

## Goal

Enable the KOReader RSS plugin to manage an optional background refresh
integration that runs after a genuine Kindle suspend/resume cycle.

Empirical testing on the target Kindle Paperwhite 4 (10th generation)
established that:

-   `com.lab126.powerd` emits `suspending "mem"` before genuine Linux
    suspend.
-   `wakeupFromSuspend` is emitted after genuine resume.
-   `outOfScreenSaver` can occur without a suspend/resume cycle, so it
    should not be used as the wake trigger.
-   `lipc-wait-event` receives `wakeupFromSuspend` after suspend.
-   A standalone `lipc-wait-event` listener exits after processing the
    wake event on this firmware.
-   An Upstart `respawn` job successfully re-arms the listener.
-   The Upstart approach was validated across two consecutive genuine
    suspend/resume cycles.

## Architecture

``` text
KOReader RSS plugin
      |
      | enable/disable background refresh
      v
manage Upstart job
      |
      v
lipc-wait-event wakeupFromSuspend
      |
      v
rss-backend refresh --reason wake
      |
      v
SQLite
```

The responsibilities should remain separated:

-   **Lua plugin:** install, remove, configure, and report status of the
    wake integration.
-   **Upstart:** wait for genuine Kindle wake events and
    supervise/re-arm the listener.
-   **Rust backend:** decide whether a refresh is appropriate and
    perform the actual network/database work.
-   **SQLite:** remain the authoritative persistent state.

KOReader itself should not need to be running when the Kindle wakes.

## 1. Rust wake-refresh semantics

The existing backend already exposes:

``` text
rss-backend --db PATH refresh [--feed ID] [--budget SEC] [--reason manual|wake]
```

Finalize the behavior of `--reason wake`.

A wake-triggered refresh should:

1.  Acquire the existing refresh lock.
2.  Exit immediately if another refresh is already running.
3.  Check when the last successful refresh occurred.
4.  Exit if the configured minimum refresh interval has not elapsed.
5.  Determine whether networking is usable.
6.  Avoid long waits if Wi-Fi is unavailable.
7.  Fetch/update feeds within the supplied time budget.
8.  Commit database changes transactionally.
9.  Record refresh metadata such as reason, result, duration, and last
    successful refresh.
10. Exit cleanly regardless of whether useful work was performed.

The backend must not keep the Kindle awake indefinitely waiting for
connectivity.

A typical invocation should remain simple:

``` sh
/mnt/us/koreader/plugins/rssreader.koplugin/bin/rss-backend \
    --db /mnt/us/koreader/data/rssreader/rss.sqlite3 \
    refresh \
    --budget 30 \
    --reason wake
```

All retry, locking, staleness, and networking policy should live in Rust
rather than in the Upstart job.

## 2. Upstart wake service

Ship an Upstart configuration template with the plugin, for example:

``` text
rssreader.koplugin/
├── main.lua
├── wake.lua
├── resources/
│   └── koreader-rss-wake.conf
└── bin/
    └── rss-backend
```

The installed service should follow the pattern empirically validated on
the PW4:

``` text
start on started lab126
stop on stopping lab126

respawn

script
    lipc-wait-event com.lab126.powerd wakeupFromSuspend

    /mnt/us/koreader/plugins/rssreader.koplugin/bin/rss-backend \
        --db /mnt/us/koreader/data/rssreader/rss.sqlite3 \
        refresh \
        --budget 30 \
        --reason wake
end script
```

Its lifecycle is:

``` text
wait for wakeupFromSuspend
        |
        v
run wake refresh
        |
        v
job exits
        |
        v
Upstart respawns job
        |
        v
wait for next wake
```

The Upstart job should contain as little policy as possible.

## 3. Lua wake-integration module

Add a dedicated module such as `wake.lua`.

Suggested interface:

``` lua
Wake.isSupported()
Wake.isInstalled()
Wake.isRunning()
Wake.install()
Wake.uninstall()
Wake.status()
```

### `isSupported()`

Check prerequisites without modifying the system:

-   `lipc-wait-event` exists.
-   `com.lab126.powerd` is available.
-   Upstart commands (`start`, `stop`, `status`) exist.
-   `/etc/upstart` exists.
-   The backend executable exists.

Unsupported devices should simply disable the UI option and provide an
explanatory status.

### `install()`

Installation should be idempotent.

Conceptual flow:

``` text
validate prerequisites
        |
        v
mntroot rw
        |
        v
install/update /etc/upstart/koreader-rss-wake.conf
        |
        v
restore mntroot ro
        |
        v
start koreader-rss-wake
        |
        v
verify service status
```

Root should be returned to read-only even if installation fails.

Do not leave the filesystem writable because an intermediate operation
failed.

### `uninstall()`

Conceptual flow:

``` text
stop koreader-rss-wake
        |
        v
mntroot rw
        |
        v
remove Upstart configuration
        |
        v
mntroot ro
```

Removal should also be idempotent.

### `status()`

Return structured information rather than requiring UI code to parse
shell output itself where practical.

Useful states include:

-   unsupported
-   not installed
-   installed/stopped
-   installed/running
-   installation needs upgrade
-   error

## 4. Plugin UI

Expose the integration as an explicit optional feature.

Example:

``` text
Background refresh

[x] Refresh after Kindle wakes

Wake service:      Running
Last wake refresh: 10:52
Last result:       Success
```

Enabling the option calls `Wake.install()`.

Disabling it calls `Wake.uninstall()`.

Do not silently modify `/etc/upstart` merely because the plugin was
installed.

The plugin should clearly distinguish:

-   manual refresh
-   wake-triggered refresh

Both should use the same Rust refresh implementation.

## 5. Installation paths

Avoid generating complex shell scripts dynamically in Lua.

Ship a reviewed Upstart template with the plugin and substitute only
values that genuinely need to vary, such as:

-   backend executable path
-   SQLite database path
-   refresh budget

All substituted shell paths must be safely quoted.

Prefer stable paths wherever possible so plugin upgrades do not require
rewriting the service unnecessarily.

## 6. Upgrade handling

Add a version marker to the installed configuration:

``` text
# KOReader RSS wake integration
# version: 1
```

The plugin can compare the bundled template version with the installed
version.

For example:

``` text
installed: 1
bundled:   2
```

The plugin can then reinstall/update the integration safely.

The Upstart/backend interface should remain deliberately stable:

``` text
rss-backend --db PATH refresh --budget N --reason wake
```

Backend upgrades should normally require only replacing `rss-backend`,
not changing the system integration.

## 7. Diagnostics

Avoid creating an additional permanent wake log unless debug logging is
explicitly enabled.

Refresh metadata should preferably be persisted through the existing
SQLite/backend model.

Useful information includes:

-   last refresh attempt
-   last successful refresh
-   trigger (`manual` or `wake`)
-   duration
-   feeds checked
-   articles added
-   failure reason

The plugin can obtain this through:

``` sh
rss-backend --db PATH status
```

System integration status can be obtained from:

``` sh
status koreader-rss-wake
```

A debug mode may optionally record wake events during development, but
it should not be necessary for normal operation.

## 8. Battery-safety requirements

Wake integration must not materially interfere with normal Kindle
suspend behavior.

In particular:

-   Do not poll for power state.
-   Do not run a permanent custom daemon when Upstart +
    `lipc-wait-event` can be used.
-   Do not use `outOfScreenSaver` as the trigger.
-   Do not wake the Kindle merely to perform an opportunistic refresh.
-   Do not indefinitely wait for Wi-Fi.
-   Do not retry continuously after network failure.
-   Respect a minimum interval between successful wake refreshes.
-   Respect the refresh time budget.
-   Allow the Kindle to return to its normal power-management lifecycle
    promptly.

RTC-triggered wakeups should remain a separate future feature and are
not required for this integration.

## 9. Testing plan

### Unit/integration tests

Test Lua logic for:

-   support detection
-   installed/not-installed detection
-   template version handling
-   idempotent installation
-   idempotent removal
-   error handling
-   restoration of read-only root state after failure

Test Rust behavior for:

-   wake refresh while stale
-   wake refresh while recently refreshed
-   lock contention
-   unavailable network
-   partial network failure
-   time-budget expiration
-   successful transaction
-   refresh metadata

### Device acceptance tests

On the PW4:

1.  Enable wake refresh from the plugin.
2.  Verify the Upstart service is running.
3.  Allow the Kindle to reach genuine `mem` suspend.
4.  Wake with the power button.
5.  Verify exactly one `reason=wake` refresh attempt.
6.  Allow the Kindle to suspend again.
7.  Repeat for several consecutive cycles.
8.  Verify the Upstart listener remains armed.
9.  Verify waking only from screensaver without actual suspend does not
    trigger refresh.
10. Verify manual refresh and wake refresh cannot run concurrently.
11. Verify unavailable Wi-Fi does not cause prolonged activity.
12. Verify disabling the feature removes/stops the service.
13. Reboot and verify the enabled service starts automatically.
14. Verify normal Kindle suspend behavior and battery consumption remain
    acceptable.

## Implementation sequence

### Phase 1 --- Rust

Finalize `refresh --reason wake` semantics:

-   locking
-   minimum refresh interval
-   bounded connectivity handling
-   refresh budget
-   metadata/status reporting

### Phase 2 --- Upstart

Create the production `koreader-rss-wake.conf` template.

Test it manually on the PW4 using the same multi-cycle procedure already
validated during investigation.

### Phase 3 --- Lua

Implement `wake.lua`:

-   support detection
-   install
-   uninstall
-   status
-   version/update handling
-   safe root filesystem handling

### Phase 4 --- UI

Add the optional **Refresh after Kindle wakes** setting and diagnostics.

### Phase 5 --- Acceptance testing

Test:

-   multiple suspend/resume cycles
-   reboot persistence
-   screensaver-only transitions
-   Wi-Fi unavailable
-   refresh already running
-   rapid wake cycles
-   plugin upgrade
-   disable/uninstall
-   battery impact

## Final design principle

The integration should follow:

``` text
Lua manages
     |
Upstart detects
     |
Rust decides and executes
     |
SQLite persists
```

This keeps the Kindle-specific power-management integration small,
observable, and replaceable while keeping RSS/network/database policy in
the existing backend.
