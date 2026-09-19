#!/bin/sh
set -eu

binary=${BACKEND_BINARY:-target/debug/rss-backend}
if [ ! -x "$binary" ]; then
    cargo build --package rss-backend
fi

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/rss-validation.XXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT INT TERM
db="$tmp_dir/fixture.sqlite3"
cache="$tmp_dir/cache"
mkdir "$cache"

"$binary" --db "$db" init-fixture-db >/dev/null

# Repeatedly terminate materialization at arbitrary short timeouts. Atomic rename
# must leave either the previous complete file or no file, never a .tmp artifact.
i=0
while [ "$i" -lt 50 ]; do
    timeout 0.001 "$binary" --db "$db" materialize 1 --cache "$cache" >/dev/null 2>&1 || true
    if find "$cache" -name '.tmp-*' -print -quit | grep -q .; then
        printf '%s\n' 'crash-boundary failure: temporary materialization file remained' >&2
        exit 1
    fi
    i=$((i + 1))
done

# Validate the database and cache after the interruption loop, then exercise
# cache reconstruction after deleting all disposable materializations.
"$binary" --db "$db" device-probe --cache "$cache" >/dev/null
rm -f "$cache"/*.html
"$binary" --db "$db" materialize 1 --cache "$cache" >/dev/null
"$binary" --db "$db" device-probe --cache "$cache" >/dev/null

printf '%s\n' 'automated validation passed: fixture, crash-boundary, cache reconstruction, and database probe'
