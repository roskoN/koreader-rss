#!/bin/sh
set -eu

KOREADER_DIR=${KOREADER_DIR:-/mnt/us/koreader}
ssh kindle sh -s -- "$KOREADER_DIR" <<'REMOTE'
set -eu
koreader_dir=$1
backend="$koreader_dir/plugins/rssreader.koplugin/bin/rss-backend"
root="/var/tmp/rss-diagnostics.$$"
db="$root/fixture.sqlite3"
cache="$root/cache"
mkdir -p "$cache"
trap 'rm -rf "$root"' EXIT INT TERM

printf '%s\n' '== backend/device inventory =='
"$backend" --version
printf '%s\n' '== fixture/database probe =='
"$backend" --db "$db" init-fixture-db
"$backend" --db "$db" device-probe --cache "$cache"
printf '%s\n' '== materialization timing =='
time "$backend" --db "$db" materialize 1 --cache "$cache" >/dev/null
"$backend" --db "$db" device-probe --cache "$cache"
printf '%s\n' '== concurrent read probes =='
"$backend" --db "$db" device-probe --cache "$cache" >/dev/null & first=$!
"$backend" --db "$db" device-probe --cache "$cache" >/dev/null & second=$!
wait "$first" "$second"
printf '%s\n' 'device diagnostics passed; UI/rendering and suspend behavior remain manual'
REMOTE
