---
name: koreader-development
description: Implement or debug the Ornith KOReader Lua plugin. Use for reader UI, Lua integration, and calls across the plugin/backend boundary.
---

# KOReader Lua plugin

The plugin is the Kindle-facing UI; SQLite and article processing remain backend
responsibilities. Locate the UI entry point, backend contract, and nearest Lua
test or analogous feature before editing.

## Rules

- Keep UI state derived from backend data; do not duplicate authoritative data
  in plugin files.
- Preserve KOReader lifecycle, event, widget, and localization conventions found
  nearby. Prefer existing helpers over new framework layers.
- Treat backend responses and stored article content as fallible: present a
  useful failure state rather than crashing the reader.
- Keep interactions responsive. Avoid parsing, image conversion, or bulk IO in
  a UI callback when backend work can do it earlier.

For a contract change, search both Rust and Lua callers, change both deliberately,
and test the smallest end-to-end path available on the Kindle/QEMU workflow.
