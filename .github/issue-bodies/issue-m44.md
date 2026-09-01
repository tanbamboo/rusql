## Goal

Combine result sets with `UNION` and `UNION ALL`.

## Category

Phase I — SQL query surface.

## Depends on

- M14 SELECT projection

## Acceptance Criteria

- [ ] `SELECT a FROM t1 UNION SELECT a FROM t2` deduplicates rows
- [ ] `UNION ALL` preserves duplicates
- [ ] Compatible column count/types; ORDER BY on union result (outer)
- [ ] `mysql-diff` union cases

## File Boundaries

- `crates/rusql-executor/**`, `crates/rusql-planner/**`

## Negative Constraints

- No `INTERSECT` / `EXCEPT`
