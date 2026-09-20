#!/bin/sh
set -eu

mode=${1:-all}
host=${KINDLE_HOST:-kindle}
ko_dir=${KOREADER_DIR:-}
package_dir=${PACKAGE_DIR:-dist/rssreader.koplugin}
target=${TARGET:-armv7-unknown-linux-musleabihf}
remote_plugin="${ko_dir}/plugins/rssreader.koplugin"

if [ -z "$ko_dir" ]; then
    echo "error: set KOREADER_DIR to the path verified on this Kindle" >&2
    exit 2
fi

ssh "$host" "test -d '$ko_dir/plugins'"

case "$mode" in
    all)
        test -d "$package_dir"
        ssh "$host" "rm -rf '$remote_plugin.new' && mkdir -p '$remote_plugin.new'"
        scp -r "$package_dir/." "$host:$remote_plugin.new/"
        ssh "$host" "set -eu; '$remote_plugin.new/bin/rss-backend' --version >/dev/null; rm -rf '$remote_plugin.old'; if test -d '$remote_plugin'; then mv '$remote_plugin' '$remote_plugin.old'; fi; if mv '$remote_plugin.new' '$remote_plugin'; then rm -rf '$remote_plugin.old'; else test ! -d '$remote_plugin.old' || mv '$remote_plugin.old' '$remote_plugin'; exit 1; fi"
        ;;
    backend)
        binary="target/${target}/release/rss-backend"
        test -x "$binary"
        ssh "$host" "test -d '$remote_plugin/bin'"
        scp "$binary" "$host:$remote_plugin/bin/rss-backend.new"
        ssh "$host" "chmod 755 '$remote_plugin/bin/rss-backend.new' && '$remote_plugin/bin/rss-backend.new' --version >/dev/null && mv '$remote_plugin/bin/rss-backend.new' '$remote_plugin/bin/rss-backend'"
        ;;
    plugin)
        test -f koreader-plugin/_meta.lua
        test -f koreader-plugin/main.lua
        ssh "$host" "test -d '$remote_plugin'"
        for file in koreader-plugin/*.lua; do
            name=$(basename "$file")
            scp "$file" "$host:$remote_plugin/$name.new"
            ssh "$host" "mv '$remote_plugin/$name.new' '$remote_plugin/$name'"
        done
        ssh "$host" "mkdir -p '$remote_plugin/feeds'"
        for file in koreader-plugin/feeds/*.opml; do
            test -f "$file" || continue
            name=$(basename "$file")
            scp "$file" "$host:$remote_plugin/feeds/$name.new"
            ssh "$host" "mv '$remote_plugin/feeds/$name.new' '$remote_plugin/feeds/$name'"
        done
        ssh "$host" "mkdir -p '$remote_plugin/resources'"
        for file in koreader-plugin/resources/*.conf; do
            test -f "$file" || continue
            name=$(basename "$file")
            scp "$file" "$host:$remote_plugin/resources/$name.new"
            ssh "$host" "mv '$remote_plugin/resources/$name.new' '$remote_plugin/resources/$name'"
        done
        scp scripts/kindle-refresh-job.sh "$host:$remote_plugin/refresh-job.sh.new"
        ssh "$host" "chmod 755 '$remote_plugin/refresh-job.sh.new' && mv '$remote_plugin/refresh-job.sh.new' '$remote_plugin/refresh-job.sh'"
        ;;
    *)
        echo "usage: $0 all|backend|plugin" >&2
        exit 2
        ;;
esac

echo "Deployed $mode. Restart KOReader to load plugin changes."
