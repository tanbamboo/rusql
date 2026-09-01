## Goal

Expand WHERE clause support to match common ORM-generated predicates.

## Category

Phase I — SQL query surface. **First agent-ready parity issue.**

## Depends on

- M20 WHERE comparisons and AND

## Acceptance Criteria

- [ ] `OR` between predicates with correct precedence vs `AND`
- [ ] `NOT expr` and `NOT IN` / `NOT LIKE`
- [ ] `LIKE 'pat%'` with `%` wildcard (no ESCAPE clause required v1)
- [ ] `BETWEEN low AND high` inclusive
- [ ] `col IN (v1, v2, v3)` literal lists
- [ ] `mysql-diff` extended WHERE suite (≥5 cases)

## File Boundaries

- `crates/rusql-executor/**`, `crates/rusql-planner/**`

## Negative Constraints

- No regex `REGEXP` in this milestone
- No full-text `MATCH AGAINST`
