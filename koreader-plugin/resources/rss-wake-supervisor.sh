#!/bin/sh
# Persistent powerd bridge.  It deliberately does no scheduling itself: the
# backend owns the absolute deadline in SQLite.
BACKEND="@BACKEND@"
DATABASE="@DATABASE@"
BUDGET="@BUDGET@"
LOG="${DATABASE%/*}/wake.log"

log() {
    printf '%s %s\n' "$(date '+%Y-%m-%dT%H:%M:%S')" "$*" >> "$LOG"
    if [ -f "$LOG" ] && [ "$(wc -c < "$LOG" 2>/dev/null)" -gt 65536 ]; then
        tail -c 32768 "$LOG" > "$LOG.new" && mv "$LOG.new" "$LOG"
    fi
}

arm() {
    output=$($BACKEND --db "$DATABASE" schedule 2>&1)
    status=$?
    log "schedule status=$status output=$output"
    [ "$status" -eq 0 ] || return 0
    seconds=$(printf '%s\n' "$output" | awk -F= '/^seconds_until=/{print $2; exit}')
    case "$seconds" in
        ''|*[!0-9]*) log "schedule invalid seconds"; return 0 ;;
    esac
    [ "$seconds" -ge 1 ] || seconds=1
    if lipc-set-prop -i -- com.lab126.powerd rtcWakeup "$seconds" >/dev/null 2>&1; then
        log "rtcWakeup accepted seconds=$seconds"
    else
        log "rtcWakeup rejected seconds=$seconds"
    fi
}

ready_listener() {
    while :; do
        event=$(lipc-wait-event com.lab126.powerd readyToSuspend 2>/dev/null)
        status=$?
        [ "$status" -eq 0 ] || { sleep 1; continue; }
        case "$event" in
            *"readyToSuspend 1"*) arm ;;
        esac
    done
}

wake_listener() {
    while :; do
        event=$(lipc-wait-event com.lab126.powerd wakeupFromSuspend 2>/dev/null)
        status=$?
        [ "$status" -eq 0 ] || { sleep 1; continue; }
        log "wakeupFromSuspend event=$event"
        # This is a bounded, temporary hint to powerd.  The trap releases it
        # even when the backend fails; the backend's budget bounds the hold.
        lipc-set-prop -i -- com.lab126.powerd deferSuspend 1 >/dev/null 2>&1
        trap 'lipc-set-prop -i -- com.lab126.powerd deferSuspend 0 >/dev/null 2>&1' EXIT INT TERM
        started=$(date +%s)
        output=$($BACKEND --db "$DATABASE" refresh --budget "$BUDGET" --reason wake 2>&1)
        result=$?
        elapsed=$(( $(date +%s) - started ))
        log "wake refresh status=$result elapsed_s=$elapsed output=$output"
        lipc-set-prop -i -- com.lab126.powerd deferSuspend 0 >/dev/null 2>&1
        trap - EXIT INT TERM
    done
}

ready_listener &
wake_listener &
wait
