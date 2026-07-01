# M4 — Secondary indexes

**Merged**: PR #13 · Issue #12

## Problem statement

M2–M3 supported `SELECT * FROM t WHERE id = 1` only via **full table scan**. As row counts grow, O(n) scans break the mental model users have of MySQL **secondary indexes** speeding point lookups.

ORMs and hand-written SQL both assume B-Tree–backed equality predicates on indexed columns. Without indexes, rusql was correct but not credible as a MySQL stand-in.

## Design space

| Option | Notes | Verdict |
|--------|-------|---------|
| Hash index | O(1) equality only; no range | Rejected for MVP |
| **B+Tree secondary index** | Standard; range-ready | **Chosen** |
| Covering index / INCLUDE | Fewer heap lookups | Deferred |

We implemented an in-crate **B+Tree** mapping `(table, column, value) → row pointer` with `CREATE INDEX idx ON t (col)` and executor fast path `scan_eq` when `WHERE col = literal` matches an indexed column.

## Internals (conceptual)

B+Trees keep keys sorted in internal nodes; leaves chain for range scans (not yet exposed in SQL). Comer (1979) explains why B+Trees dominate disk databases: shallow tree, sequential leaf scans.

rusql’s MVP tree is in-memory, single-column, non-unique. Duplicate keys append row ids. **No balance policy documentation for users** — invariants enforced by tests in `btree_index` module.

## Trade-offs

- Only **one column** per index; no composite `(a,b)` keys (roadmap).
- Optimizer does not cost model — if `scan_eq` matches, use index; else heap scan.
- Index metadata not yet in `information_schema.STATISTICS` (M27).

## Harness lesson

Pair **unit tests on the tree** with a **compat fixture** using `WHERE` on indexed column — catches regressions in both isolation and wire path.

## Further reading

- Comer — B-Tree survey ([Bibliography](../appendix/bibliography.md))
- Lehman & Yao — concurrent B-Tree (future work)
