#!/bin/sh
# The Rust backend is the persistent powerd bridge. It is frozen with userspace
# during suspend and resumes on the RTC/user wake event.
BACKEND="@BACKEND@"
DATABASE="@DATABASE@"
BUDGET="@BUDGET@"
exec "$BACKEND" --db "$DATABASE" powerd-daemon --budget "$BUDGET" --settle 15
