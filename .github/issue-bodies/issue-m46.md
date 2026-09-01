## Goal

Evaluate SQL expressions in SELECT, WHERE, and ORDER BY including built-in functions.

## Category

Phase I — SQL query surface.

## Depends on

- M43 GROUP BY (for some aggregates), M45 extended WHERE

## Acceptance Criteria

- [ ] Arithmetic `+ - * /` on numeric columns/literals
- [ ] String concat `CONCAT(a, b)` or `||` if MySQL mode
- [ ] `COALESCE`, `IFNULL`, `NULLIF`
- [ ] `CAST(expr AS type)` for supported types
- [ ] `NOW()`, `CURDATE()`, `LENGTH()`, `LOWER()`, `UPPER()` minimum set
- [ ] `mysql-diff` expression cases

## File Boundaries

- `crates/rusql-executor/**`, `crates/rusql-sql/**`, `crates/rusql-i18n/**`

## Negative Constraints

- No user-defined functions (see M47)
- No JSON functions until JSON type stable (M40)
