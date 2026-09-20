#!/bin/sh
set -eu

target=${TARGET:-armv7-unknown-linux-musleabihf}
package_dir=${PACKAGE_DIR:-dist/rssreader.koplugin}
binary="target/${target}/release/rss-backend"

test -x "$binary"
test -f koreader-plugin/_meta.lua
test -f koreader-plugin/main.lua

rm -rf "$package_dir"
mkdir -p "$package_dir/bin"
mkdir -p "$package_dir/resources"
mkdir -p "$package_dir/feeds"
cp koreader-plugin/*.lua "$package_dir/"
cp koreader-plugin/resources/*.conf "$package_dir/resources/"
cp koreader-plugin/feeds/.gitkeep "$package_dir/feeds/"
cp scripts/kindle-refresh-job.sh "$package_dir/refresh-job.sh"
chmod 755 "$package_dir/refresh-job.sh"
for file in koreader-plugin/feeds/*.opml; do
    test -f "$file" || continue
    cp "$file" "$package_dir/feeds/"
done
cp "$binary" "$package_dir/bin/rss-backend"
chmod 755 "$package_dir/bin/rss-backend"

echo "Packaged $package_dir"
