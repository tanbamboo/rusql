## Goal

Implement MySQL `CASE` expressions and `IF(cond, then, else)` so ORMs and application SQL can branch without falling back to client-side logic.

## Background

Phase Q after M65. `CASE` / `IF` are among the highest-ROI SQL gaps for ORM-generated queries and reporting SQL. Currently `eval_expr` returns `unsupported expression` for `Expr::Case`.

## Acceptance Criteria

- [ ] Searched `CASE`: `CASE WHEN c THEN a WHEN d THEN b ELSE e END`
- [ ] Simple `CASE`: `CASE expr WHEN v1 THEN a WHEN v2 THEN b ELSE e END`
- [ ] `IF(cond, then_expr, else_expr)` as a built-in function
- [ ] Works in `SELECT` projection (with and without `FROM`) and in `WHERE` predicates where expressions are evaluated
- [ ] NULL / empty-string nullish handling consistent with existing COALESCE path
- [ ] Unit tests in `rusql-executor` + wire test in `rusql-server`
- [ ] `mysql-diff` suite covering CASE/IF against Docker MySQL 8.0
- [ ] Docs: CHANGELOG, release-notes (en+zh-CN), user-guide (en+zh-CN), roadmap Phase Q row

## File Boundaries

Allowed:
- `crates/rusql-executor/src/expr.rs`
- `crates/rusql-executor/src/lib.rs` (only if WHERE needs extra wiring)
- `crates/rusql-executor/src/where_filter.rs` (only if needed)
- `crates/rusql-server/src/**` (tests)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Storage / WAL changes
- Protocol changes
- New crates / deps

## Negative Constraints

- No `CASE` in DDL defaults
- No window-function `CASE` over partitions (out of scope)
- Do not implement `IFNULL` again (already exists)

## Test plan

```bash
cargo test -p rusql-executor case
cargo test -p rusql-server case_if
node scripts/mysql-diff.mjs
```
