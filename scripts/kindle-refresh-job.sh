#!/bin/sh
# One-shot unattended refresh entrypoint. Invoke this from a verified Kindle
# wake/powerd mechanism; it intentionally does not install a daemon or alter
# power-management settings.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ -x "$script_dir/bin/rss-backend" ]; then
    default_plugin_dir=$script_dir
else
    default_plugin_dir=$script_dir/../koreader-plugin
fi
plugin_dir=${RSS_PLUGIN_DIR:-$default_plugin_dir}
data_dir=${RSS_DATA_DIR:-/mnt/us/koreader/data/rssreader}
cache_dir=${RSS_CACHE_DIR:-/mnt/us/koreader/cache/rssreader}
backend=${RSS_BACKEND:-$plugin_dir/bin/rss-backend}
budget=${RSS_REFRESH_BUDGET:-60}

mkdir -p "$data_dir" "$cache_dir"

# A wake can precede Wi-Fi readiness. Probe a bounded number of times, then
# leave the persisted scheduler to retry on the next wake if connectivity is
# unavailable.
ready=0
i=0
while [ "$i" -lt 12 ]; do
    if "$backend" http-probe --url https://example.com/ >/dev/null 2>&1; then
        ready=1
        break
    fi
    i=$((i + 1))
    sleep 5
done
if [ "$ready" -ne 1 ]; then
    exit 0
fi

"$backend" --db "$data_dir/rss.sqlite3" refresh --reason wake --budget "$budget"
