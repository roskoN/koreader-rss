#!/bin/sh
set -eu

target=${TARGET:-armv7-unknown-linux-musleabihf}
binary="target/${target}/release/rss-backend"

test -x "$binary"
file "$binary"

if readelf -l "$binary" | grep -q 'interpreter'; then
    echo "error: ARM binary has a dynamic interpreter" >&2
    exit 1
fi
if readelf -d "$binary" 2>/dev/null | grep -q '(NEEDED)'; then
    echo "error: ARM binary has dynamic dependencies" >&2
    exit 1
fi

qemu-arm -cpu cortex-a9 "$binary" --version
qemu-arm -cpu cortex-a9 "$binary" doctor
if [ -n "${ARM_TLS_URL:-}" ]; then
    qemu-arm -cpu cortex-a9 "$binary" http-probe --url "$ARM_TLS_URL"
fi

tmp_dir=$(mktemp -d /tmp/rss-backend-arm.XXXXXX)
trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM
qemu-arm -cpu cortex-a9 "$binary" init-probe-db --db "$tmp_dir/probe.sqlite3"
qemu-arm -cpu cortex-a9 "$binary" materialize-fixture --out "$tmp_dir/fixture.html"
test -s "$tmp_dir/probe.sqlite3"
test -s "$tmp_dir/fixture.html"
grep -q 'data:image/png;base64,' "$tmp_dir/fixture.html"
grep -q 'data:image/jpeg;base64,' "$tmp_dir/fixture.html"
