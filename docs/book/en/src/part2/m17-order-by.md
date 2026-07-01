# M17 — SELECT ORDER BY

**Issue #40**

## Problem

Clients assume stable, user-defined row order. Heap scans and index lookups return insertion order, which breaks ORMs, reports, and `ORDER BY` in compat suites. MySQL applies sort after projection and filter; rusql must do the same for basic table `SELECT`.

## Design space

| Approach | Pros | Cons |
|----------|------|------|
| Sort in executor after scan | Simple, matches current LIMIT/projection pipeline | No index-order optimization |
| Push sort to storage/index | Fast for indexed columns | Needs planner + sort order metadata |
| External sort on disk | Scales to large tables | Overkill for MVP heap |

## Decision

- Read `Query.order_by` from sqlparser (MySQL places `ORDER BY` on the outer `Query`).
- Resolve sort keys against **output** column names (post-projection).
- Lexicographic string compare on cell values; `ASC` default, `DESC` reverses.
- Apply **after** filter/projection, **before** `LIMIT` (MySQL semantics).

## Internals

```
scan → project → WHERE filter → ORDER BY sort → LIMIT take → result set
```

No `NULLS FIRST/LAST`, multi-column sort with expressions, or `ORDER BY` on literals only.

## Further reading

- MySQL 8.0 Reference: [ORDER BY optimization](https://dev.mysql.com/doc/refman/8.0/en/order-by-optimization.html)
- Graefe, *Query evaluation techniques for large databases* (1993) — sort vs index-order trade-offs

## Harness lesson

> Add `ORDER BY` cases beside existing `LIMIT` rows in `basic.json` so wire compat catches ordering regressions without manual CLI checks.
