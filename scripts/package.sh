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
cp koreader-plugin/*.lua "$package_dir/"
cp "$binary" "$package_dir/bin/rss-backend"
chmod 755 "$package_dir/bin/rss-backend"

echo "Packaged $package_dir"
