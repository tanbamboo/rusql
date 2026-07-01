# M9 — Transactions

**Merged**: PR #22 · Issue #19 · [spec](../../../en/specs/m9-transactions.md)

## Problem

Concurrent connections need **isolation** for uncommitted work; users expect `BEGIN` / `COMMIT` / `ROLLBACK`.

## Design choices

- Per-connection **transaction overlay** on storage engine
- Uncommitted writes invisible to other connections
- `COMMIT` flushes to WAL; `ROLLBACK` discards overlay

## Trade-offs

Not full MVCC — single-writer overlay MVP. Good enough for wire semantics and teaching harness scope control.

## Harness lesson

> Large semantic features still ship as **one milestone** if file boundaries and compat tests are defined upfront.

## See also

- [m9-transactions.md](../../../en/specs/m9-transactions.md)
