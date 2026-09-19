# Kindle RTC Wake Experiment

This experiment determines whether the Kindle can wake from suspend using its
standard RTC alarm path and then run the deployed background refresh wrapper.
It is intentionally temporary and must not install a recurring schedule.

## Safety and scope

- Run only on the test Kindle.
- Use a short interval, initially 120 seconds.
- Record battery, Wi-Fi, and powerd state before and after.
- Do not change `preventScreenSaver`, `deferSuspend`, or other keep-awake
  properties.
- Clear the temporary RTC alarm after the experiment.
- Host and QEMU cannot establish wake or battery behavior.

## 1. Connect and record baseline

```sh
ssh kindle
date -u
lipc-get-prop com.lab126.powerd state
lipc-get-prop com.lab126.powerd status
lipc-probe -a | grep -A20 com.lab126.powerd
```

Record the battery level, Wi-Fi state, firmware, KOReader version, and current
powerd status in the observation log.

## 2. Start a temporary resume marker

```sh
rm -f /var/tmp/rss-wake-marker
(
    sleep 120
    date -u > /var/tmp/rss-wake-marker
) &
```

The marker should be written only after the process resumes. Its timestamp is
not proof of a hardware wake by itself; compare it with the suspend interval
and RTC alarm result.

## 3. Program an RTC alarm without suspending

```sh
rtcwake -m no -s 120
cat /proc/driver/rtc
```

Confirm that `/proc/driver/rtc` reports a future alarm. `-m no` programs the
alarm without changing the current power state.

## 4. Suspend manually

Use the Kindle's normal power/sleep action. Do not modify powerd properties.
The SSH connection may disconnect. Wait at least two minutes before attempting
to reconnect.

After reconnecting:

```sh
cat /var/tmp/rss-wake-marker
date -u
lipc-get-prop com.lab126.powerd state
lipc-get-prop com.lab126.powerd status
```

Record whether the Kindle woke by itself, the wake delay, Wi-Fi readiness, and
the battery delta.

## 5. Run the bounded refresh wrapper

After Wi-Fi is usable:

```sh
/mnt/us/koreader/plugins/rssreader.koplugin/refresh-job.sh
cat /mnt/us/koreader/data/rssreader/logs/refresh.log
/mnt/us/koreader/plugins/rssreader.koplugin/bin/rss-backend \
  --db /mnt/us/koreader/data/rssreader/rss.sqlite3 status
```

Confirm that the wrapper exits within its budget, records a `wake` refresh,
and leaves no process or keep-awake state behind.

## 6. Clear the temporary alarm

```sh
rtcwake -m disable
rm -f /var/tmp/rss-wake-marker
```

Verify normal suspend behavior after cleanup.

## 7. Repeat and classify

Repeat the experiment at least three times. Record:

| Run | Alarm set | Device woke | Wi-Fi ready (s) | Refresh duration (s) | Battery delta | Resuspended normally |
|---|---|---|---:|---:|---:|---|
| 1 | | | | | | |
| 2 | | | | | | |
| 3 | | | | | | |

Classify the mechanism as `VERIFIED` only if wake, Wi-Fi readiness, bounded
refresh, cleanup, and normal resuspend are repeatable. If `rtcwake` does not
work with Kindle suspend, investigate the firmware-specific `powerd` LIPC
client protocol separately; do not guess the `rtcWakeup2` string format.
