# M2 — COM_QUERY

**Merged**: PR #9 · Issues #3, #4 (ADRs)

## Problem

After handshake, clients send SQL as **`COM_QUERY`**. We needed parsing, catalog, executor, and storage for a minimal DML loop.

## Design choices

| Layer | Choice |
|-------|--------|
| SQL | `sqlparser` MySQL dialect ([adr-sql-parser](../../../en/specs/adr-sql-parser.md)) |
| Catalog | In-memory `TableMeta` in `rusql-core` |
| Storage | `StorageEngine` trait + heap engine |
| Execution | Pass-through planner, volcano-style executor |

Supported: `CREATE TABLE`, `INSERT … VALUES`, `SELECT *`, `SELECT` literal.

## Trade-offs

- **No query optimizer** — acceptable for MVP; planner crate is a seam for later.
- **Stringly-typed column types** from parser `Display` — simplified DESCRIBE later (M12).

## What we deferred

Indexes, persistence, auth, prepared statements.

## Harness lesson

> Lock parser and crate boundaries in **ADRs before M2** so agents do not swap `sqlparser` mid-stream.

## See also

- [adr-sql-parser.md](../../../en/specs/adr-sql-parser.md)
