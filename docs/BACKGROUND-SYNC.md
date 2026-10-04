# Background Synchronization

## Overview

Feed refresh work remains bounded. The KOReader plugin schedules its persisted
deadline through `Device.wakeup_mgr`; no standalone event listener or RTC alarm
writer is used.

The persisted scheduler stores each feed's `next_due_at` and selects enabled,
due feeds in deterministic schedule order. Each feed and its articles are
processed sequentially; refresh progress and outcomes are stored in
`refresh_runs`. The database retains only the latest ten refresh runs; the most
recent successful refresh timestamp is persisted separately in `app_state` so
it survives history pruning. The plugin's **Refresh status (last 10)** entry
displays the retained history. A completed manual pass advances the global wake
deadline by one configured interval to avoid repeating a just-finished pass;
individual failed feeds retain their own retry deadlines.

Network inputs are capped at 1 MiB per feed, 2 MiB per article page, and 2 MiB
per source image. Image decoding additionally rejects images above 4 million
pixels, caps decoder allocations at 32 MiB, and embeds no more than 4 MiB of
image data per article. An image rejected by these limits is omitted while its
alt text and article text remain; it is not allowed to trigger an unbounded
full-resolution decode.

## Wake refresh trigger

At startup and after refresh, Lua reads the backend's `schedule` output and
registers a single callback-based task with `Device.wakeup_mgr`. KOReader owns
the Kindle suspend/resume integration and invokes the callback only after it
validates a scheduled wake. The callback launches bounded
`refresh --reason wake`; normal user resumes do not trigger refresh. See
[`WAKE-INTEGRATION.md`](WAKE-INTEGRATION.md) for details.

Physical suspend/resume execution, normal return to autosuspend, Wi-Fi readiness,
and battery impact remain `NEEDS EXPERIMENT` on Kindle. Host or QEMU results do
not establish those behaviors. The retired `powerd-daemon` and Upstart
implementation are not part of the current plugin/backend.

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
