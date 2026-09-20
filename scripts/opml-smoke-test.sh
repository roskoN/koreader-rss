#!/bin/sh
set -eu

binary=${BACKEND_BINARY:-target/debug/rss-backend}
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
if [ -z "${BACKEND_BINARY:-}" ]; then
    cargo build --package rss-backend
fi

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/rss-opml-smoke.XXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT INT TERM
db="$tmp_dir/feeds.sqlite3"
opml_dir="$tmp_dir/opml"
mkdir "$opml_dir"
cp "$root/assets/test-opml.opml" "$opml_dir/subscriptions.opml"

expected=$(awk -F 'xmlUrl=' 'NF > 1 { count += NF - 1 } END { print count + 0 }' "$opml_dir/subscriptions.opml")
imported=$($binary --db "$db" feed import-opml "$opml_dir")
case "$imported" in
    imported=*) : ;;
    *) printf 'OPML import failed: %s\n' "$imported" >&2; exit 1 ;;
esac
actual=${imported#imported=}
[ "$actual" -eq "$expected" ] || {
    printf 'OPML import count mismatch: expected=%s actual=%s\n' "$expected" "$actual" >&2
    exit 1
}

# The OPML contains a broad real-world subscription set. Import every entry,
# then retrieve from two stable feeds from that same file so this test remains
# finite while exercising full article retrieval (including images/extraction).
for row in $($binary --db "$db" feed list | awk -F '\t' '$4 != "https://feeds.arstechnica.com/arstechnica/index" && $4 != "https://www.theverge.com/rss/index.xml" { print $1 }'); do
    $binary --db "$db" feed disable "$row"
done
$binary --db "$db" refresh --unbounded >/dev/null
status=$($binary --db "$db" status)
new_articles=$(printf '%s\n' "$status" | awk -F= '$1 == "new_articles" { print $2; found=1 } END { if (!found) print 0 }')
[ "$new_articles" -gt 0 ] || {
    printf 'OPML refresh retrieved no articles\n%s\n' "$status" >&2
    exit 1
}

printf 'OPML smoke test passed: feeds=%s new_articles=%s\n' "$actual" "$new_articles"
