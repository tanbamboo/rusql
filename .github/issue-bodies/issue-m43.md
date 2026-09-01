## Goal

Implement `GROUP BY`, `HAVING`, and common aggregate functions.

## Category

Phase I — SQL query surface.

## Depends on

- M14 SELECT projection, M20 WHERE

## Acceptance Criteria

- [ ] `GROUP BY col` with `COUNT(*)`, `COUNT(col)`, `SUM`, `MIN`, `MAX`, `AVG`
- [ ] `HAVING agg_expr op literal` filters groups
- [ ] SELECT list rules: grouped columns + aggregates (MySQL ONLY_FULL_GROUP_BY subset)
- [ ] `mysql-diff` aggregate cases match MySQL 8.0 output

## File Boundaries

- `crates/rusql-executor/**`, `crates/rusql-planner/**`

## Negative Constraints

- No `GROUP_CONCAT`, `JSON_ARRAYAGG` in v1
- No window functions (`OVER`)
