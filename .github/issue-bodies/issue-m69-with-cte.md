## Goal

Implement non-recursive `WITH` common table expressions so a named subquery can be referenced in `FROM` (MySQL 8.0 CTE MVP).

## Background

Phase Q after M68. Derived tables (`FROM (SELECT …) AS d`) already work (M42). Non-recursive `WITH cte AS (SELECT …) SELECT … FROM cte` is the next ORM/reporting gap. Recursive CTEs and window functions are out of scope.

## Acceptance Criteria

- [ ] `WITH cte AS (SELECT …) SELECT … FROM cte` returns the CTE rows
- [ ] Multiple CTEs: later CTEs may reference earlier ones
- [ ] CTE works with `WHERE`, projection, `ORDER BY` / `LIMIT` on the outer query
- [ ] `WITH cte AS (…) INSERT INTO t SELECT … FROM cte` works (reuse M68 INSERT SELECT)
- [ ] `WITH RECURSIVE` returns a clear unsupported error
- [ ] Unknown CTE vs missing table still reports table-not-found
- [ ] Unit/wire tests + `mysql-diff` suite
- [ ] Docs: CHANGELOG, release-notes, user-guide (en+zh-CN), roadmap Phase Q, HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**`
- `crates/rusql-i18n/**`
- `crates/rusql-server/src/**` (tests)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Recursive CTE evaluation
- Window functions (`OVER`, `ROW_NUMBER`, `RANK`)
- Storage redesign
- New crates

## Negative Constraints

- Do not implement `WITH RECURSIVE`
- Do not implement `AS MATERIALIZED` / `NOT MATERIALIZED` semantics beyond parse-and-ignore or a clear error
- Do not change existing derived-table behavior

## Test plan

```bash
cargo test -p rusql-sql parse_with
cargo test -p rusql-executor with_cte
cargo test -p rusql-server with_cte
node scripts/mysql-diff.mjs
```
