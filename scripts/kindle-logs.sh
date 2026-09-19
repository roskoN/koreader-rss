#!/bin/sh
set -eu

host=${KINDLE_HOST:-kindle}
ko_dir=${KOREADER_DIR:-}

if [ -z "$ko_dir" ]; then
    echo "error: set KOREADER_DIR after verifying the installed KOReader path" >&2
    exit 2
fi

ssh "$host" "test ! -f '$ko_dir/crash.log' || tail -n 200 '$ko_dir/crash.log'"
