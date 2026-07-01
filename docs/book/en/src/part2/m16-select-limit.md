# M16 — SELECT LIMIT

**Merged**: PR #39 · Issue #38

## Problem

ORMs, REST APIs, and admin UIs paginate with `LIMIT`. Without it every query returns the full heap scan — unusable for large tables and incompatible with common MySQL client expectations (`LIMIT 1` for existence checks).

## Design space

| Approach | Pros | Cons |
|----------|------|------|
| `take(n)` after full materialization | Simple; correct with WHERE/projection | No early termination on scan |
| Push `LIMIT` into index/scan | Faster | Needs planner awareness |
| `LIMIT` in protocol only | — | Wrong semantics |

## Decision

- Read `Query.limit` from sqlparser AST (outer `Query`, not inner `Select`).
- Apply `take(n)` **after** scan, projection, filter, and (later) `ORDER BY`.
- Integer literal limits only in MVP.

## Internals

```
… → filter → project → ORDER BY (M17) → apply_limit(take n) → rows
```

`information_schema` scans share the same `finish_rows_query` path.

## Trade-offs

No `OFFSET`, no optimizer pushdown — acceptable for small heaps. `LIMIT` with `ORDER BY` ordering defined in M17.

## Further reading

- MySQL 8.0 Reference: [LIMIT clause](https://dev.mysql.com/doc/refman/8.0/en/select.html)
- SQLite pagination patterns (offset/limit semantics comparison)

## Harness lesson

> Pagination tests belong in **compat JSON** next to existing `basic_dml` inserts — one `LIMIT 1` row asserts column order is preserved.
