---
name: sqlite-storage
description: Change or investigate Ornith's SQLite authoritative store. Use for schema, queries, migrations, transactional behavior, and stored article BLOBs.
---

# SQLite storage

SQLite is the authoritative persistent store for feeds, articles, processing
state, and materialized content. Find schema creation/migrations, the query
owner, and all call sites before changing a table or query.

## Invariants

- Migrations must be forward-safe for an existing on-device database; preserve
  user data and transaction boundaries.
- Store materialized articles as compressed, self-contained HTML BLOBs. Their
  images are grayscale/downscaled and Base64 embedded, not external runtime
  dependencies.
- Use stable identifiers and explicit ordering. Do not rely on incidental row
  order or filesystem state.
- Bound query result sizes and avoid N+1 access in feed/article lists.

Test migration and query behavior with a minimal temporary database. Inspect
the actual rows/BLOB metadata only as needed; do not dump whole article bodies
into context or logs.
