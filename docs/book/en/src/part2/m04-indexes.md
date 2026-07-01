# M4 — Secondary indexes

**Merged**: PR #13 · Issue #12

## Problem

`SELECT … WHERE col = literal` on heap tables is O(n). MySQL users expect index-backed point lookups.

## Design choices

- In-crate **B+Tree** secondary index on one column
- `CREATE INDEX idx ON tbl (col)` in executor
- `scan_eq` fast path when predicate matches indexed column

## Trade-offs

- Single-column secondary indexes only
- No composite keys, no covering indexes, no optimizer cost model

## CI note

First **rustfmt** cross-platform failure on this PR — now a known harness tax (run fmt locally).

## Harness lesson

> Add **unit tests on btree** plus one **compat fixture** with `WHERE` — dual-layer feedback.

## See also

- `crates/rusql-storage` btree module
