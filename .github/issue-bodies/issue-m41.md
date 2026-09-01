## Goal

Support `LEFT JOIN` and `RIGHT JOIN` with NULL-padding for non-matching rows.

## Category

Phase I — SQL query surface.

## Depends on

- M22 INNER JOIN

## Acceptance Criteria

- [ ] `SELECT … FROM a LEFT JOIN b ON …` returns unmatched `a` rows with NULL `b` columns
- [ ] `RIGHT JOIN` equivalent behavior
- [ ] Works with `WHERE`, `ORDER BY`, `LIMIT` on joined result
- [ ] `mysql-diff` portable suite with outer join cases

## File Boundaries

- `crates/rusql-executor/**`, `crates/rusql-planner/**`

## Negative Constraints

- No FULL OUTER JOIN in this milestone
- No nested-loop vs hash join optimizer (M49)
