# M14 — SELECT column projection

**Merged**: PR #35 · Issue #34

## Problem

ORMs and application code rarely need every column on every query. Returning full rows for `SELECT id FROM users` wastes wire bandwidth, obscures which fields clients actually depend on, and hides projection bugs until production traffic hits.

Worse, many frameworks issue narrow `SELECT` lists during hydration; if the engine only supports `*`, drivers may over-fetch or fail compatibility checks against real MySQL.

## Design space

| Approach | Pros | Cons |
|----------|------|------|
| Project in executor after scan | Reuses scan/index paths; simple | Cannot push projection into storage yet |
| Project in storage layer | Less memory on wide tables | Couples catalog to every engine API |
| Always `SELECT *` internally | Minimal code | Wrong semantics; breaks compat |

## Decision

- Resolve the `SELECT` list against catalog column order via `resolve_projection`.
- Project rows **after** heap scan or index lookup (same paths as `SELECT *`).
- `Wildcard` (`*`) keeps pass-through behavior with `proj_indices = None`.
- Output column names follow identifiers or explicit aliases (`SELECT id AS user_id`).

## Internals

```
scan/index → finalize_select_rows → project_rows(indices) → result columns
```

Projection indices map output positions to underlying table column indexes. `WHERE` still evaluates against **table** column names before projection.

## Trade-offs

Identifiers only — no expressions, aggregates, or `COUNT(*)` yet. Ordering and `LIMIT` (M16–M17) apply to projected rows.

## Further reading

- Graefe, *Query evaluation techniques for large databases* (1993) — early/late projection trade-offs
- MySQL 8.0 Reference: [SELECT statement](https://dev.mysql.com/doc/refman/8.0/en/select.html)

## Harness lesson

> Extend the **existing compat suite** (`basic_dml`) with one projected query — cheapest regression signal for executor changes.

## See also

- [m14-select-projection.md](../../../en/specs/m14-select-projection.md)
