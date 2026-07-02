# M3 — WAL persistence

**Merged**: PR #11 · Issue #10

## Problem statement

After M2, rusql held tables entirely in memory. That is sufficient for unit tests but fails the first user expectation of a *database*: **survival across process restarts**. MySQL clients and operators assume `mysqld` can stop and start without losing committed tables.

The failure mode is subtle in development: integration tests pass in one process, then a manual `mysql` session after restart shows an empty catalog. We needed **durable representation** of DDL and DML without yet building a full page-based storage engine.

## Design space

| Approach | Pros | Cons | Verdict |
|----------|------|------|---------|
| Snapshot files (JSON/bincode) | Simple | Rewrite whole DB; awkward concurrency | Deferred |
| Append-only **WAL** | Industry standard; replay on startup | Requires log format + recovery logic | **Chosen (MVP)** |
| Full page store + buffer pool | MySQL/InnoDB path | Large scope | Roadmap M32+ |

We adopted a **minimal WAL**: append records for catalog mutations and row inserts; replay reconstructs heap + `Catalog` on startup.

## Implementation sketch

On `CREATE TABLE` / `INSERT`, the executor appends typed records to `rusql.wal` under `--data-dir`. Startup reads the log sequentially:

1. Replay `CreateTable` into storage + session catalog
2. Replay `Insert` rows into heap

`ColumnDef` gained `Serialize` so catalog snapshots embed in WAL entries — a real bug caught pre-merge when serde was missing.

This is **not** ARIES: we do not yet write CLR records, undo chains, or fuzzy checkpoints. Mohan et al. (1992) describe the full protocol rusql may approach in M31 (durable `COMMIT`).

## Trade-offs

- **Crash recovery**: Replay only what was appended; no torn-page handling (single file append is atomic enough for MVP).
- **Transactions**: M9 added a connection overlay; M31 verified `COMMIT` durability across WAL replay and `ROLLBACK` non-flush.
- **Performance**: Full replay on every start — acceptable for dev datasets.

## Incident: harness feedback

Parallel tests reused a temp data directory → flaky `persistence_across_connections`. The failure was **test isolation**, not WAL logic — but CI taught us to treat data-dir paths like shared mutable state. Fix: per-test unique directories.

## Harness lesson

Persistence requires **restart integration tests** as a first-class sensor, not an optional manual step. Add them in the same PR as WAL write path.

## Further reading

- Mohan et al. — ARIES (see [Bibliography](../appendix/bibliography.md))
- rusql spec: user-guide persistence section
- Roadmap M31 — durable transaction commit
