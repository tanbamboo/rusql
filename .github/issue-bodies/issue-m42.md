## Goal

Execute correlated and uncorrelated subqueries in SELECT and WHERE clauses.

## Category

Phase I — SQL query surface.

## Depends on

- M45 extended WHERE, M22 JOIN

## Acceptance Criteria

- [ ] `WHERE col IN (SELECT …)` semi-join semantics
- [ ] `WHERE EXISTS (SELECT …)` / `NOT EXISTS`
- [ ] Scalar subquery in SELECT list `(SELECT COUNT(*) FROM …)`
- [ ] Derived tables `FROM (SELECT …) AS alias`
- [ ] Portable `mysql-diff` cases; no MySQL error for supported subset

## File Boundaries

- `crates/rusql-executor/**`, `crates/rusql-planner/**`, `crates/rusql-sql/**`

## Negative Constraints

- No lateral derived tables
- No recursive CTE
