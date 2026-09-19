#!/bin/sh
set -eu

host=${KINDLE_HOST:-kindle}
ko_dir=${KOREADER_DIR:-}

if [ -z "$ko_dir" ]; then
    echo "error: set KOREADER_DIR after verifying the installed KOReader path" >&2
    exit 2
fi

ssh "$host" "set -eu; \
echo '=== uname ==='; uname -a; \
echo '=== model ==='; test ! -r /proc/device-tree/model || tr '\\000' ' ' </proc/device-tree/model; echo; \
echo '=== cpu ==='; cat /proc/cpuinfo; \
echo '=== memory ==='; cat /proc/meminfo; \
echo '=== storage ==='; df -k; \
echo '=== mounts ==='; mount; \
echo '=== framebuffer ==='; for f in /sys/class/graphics/fb0/virtual_size /sys/class/graphics/fb0/bits_per_pixel /sys/class/graphics/fb0/modes; do test ! -r \"\$f\" || { printf '%s=' \"\$f\"; cat \"\$f\"; }; done; \
echo '=== backend ==='; '$ko_dir/plugins/rssreader.koplugin/bin/rss-backend' --version; '$ko_dir/plugins/rssreader.koplugin/bin/rss-backend' doctor; \
echo '=== https ==='; '$ko_dir/plugins/rssreader.koplugin/bin/rss-backend' http-probe --url https://example.com/"
