#!/bin/sh
set -eu

binary=${BACKEND_BINARY:-target/release/rss-backend}
cargo build --release --package rss-backend
tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/rss-benchmark.XXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT INT TERM
db="$tmp_dir/fixture.sqlite3"
cache="$tmp_dir/cache"
mkdir "$cache"

"$binary" --db "$db" init-fixture-db >/dev/null
printf '%s\n' 'materialize benchmark (100 iterations):'
/usr/bin/time -f 'elapsed=%e s max_rss=%M KB' sh -c '
    i=0
    while [ "$i" -lt 100 ]; do
        "$1" --db "$2" materialize 1 --cache "$3" >/dev/null
        i=$((i + 1))
    done
' sh "$binary" "$db" "$cache"
"$binary" --db "$db" device-probe --cache "$cache"
