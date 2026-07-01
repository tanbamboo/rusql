# M3 — WAL persistence

**Merged**: PR #11 · Issue #10

## Problem

In-memory heap loses data on restart. Users expect `--data-dir` durability like any database.

## Design choices

- Append-only **WAL** file (`rusql.wal`) under configurable data directory
- Replay on server start into heap + catalog
- `ColumnDef` serde for catalog snapshots in WAL records

## Trade-offs

- **WAL skeleton**, not full ARIES — enough for CREATE/INSERT replay
- Transactions deferred to M9 (overlay instead of WAL-level txn)

## Incident (harness feedback)

Parallel tests shared a temp directory → flaky `persistence_across_connections`. Fixed with per-test isolated data dirs — **feedback caught a test harness bug, not production logic**.

## Harness lesson

> Persistence milestones need **restart integration tests**; isolate temp paths when tests run in parallel.

## See also

- User guide: persistence section
