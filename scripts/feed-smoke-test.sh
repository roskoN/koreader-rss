#!/bin/sh
set -eu

binary=${BACKEND_BINARY:-target/debug/rss-backend}
if [ -z "${BACKEND_BINARY:-}" ]; then
    cargo build --package rss-backend
fi

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/rss-feed-smoke.XXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT INT TERM
db="$tmp_dir/feeds.sqlite3"

feeds='https://www.theverge.com/rss/index.xml
https://feeds.arstechnica.com/arstechnica/index'

printf '%s\n' "$feeds" | while IFS= read -r url; do
    [ -n "$url" ] || continue
    output=$($binary --db "$db" feed check "$url")
    case "$output" in
        ok*)
            case "$output" in
                *'entries=0'*)
                    printf 'feed returned no entries: %s\n' "$url" >&2
                    exit 1
                    ;;
            esac
            printf 'feed check passed: %s (%s)\n' "$url" "$output"
            ;;
        *)
            printf 'feed check failed: %s\n%s\n' "$url" "$output" >&2
            exit 1
            ;;
    esac
done

printf '%s\n' 'live feed smoke tests passed'
