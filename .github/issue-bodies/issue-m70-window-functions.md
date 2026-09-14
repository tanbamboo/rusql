## Goal

Implement a MySQL-compatible window-function MVP: `ROW_NUMBER()`, `RANK()`, and `DENSE_RANK()` with `OVER (PARTITION BY … ORDER BY …)`.

## Background

Phase Q after M69 CTEs. Reporting/ORM SQL often needs numbered rows per group. Aggregates already exist (M43); this issue adds windowed ranking without collapsing rows.

## Acceptance Criteria

- [ ] `SELECT id, ROW_NUMBER() OVER (ORDER BY id) AS n FROM t` numbers rows
- [ ] `PARTITION BY col` restarts numbering per partition
- [ ] `RANK()` and `DENSE_RANK()` follow MySQL tie semantics
- [ ] Works with `WHERE` (applied before the window) and outer `ORDER BY` / `LIMIT`
- [ ] Unsupported frames (`ROWS BETWEEN …`) return a clear error
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
- Full analytic function set (`LAG`/`LEAD`/`SUM() OVER` beyond this issue)
- Window frames (`ROWS`/`RANGE`)
- Storage redesign
- New crates

## Negative Constraints

- Do not implement `ROWS BETWEEN` / `RANGE BETWEEN`
- Do not implement aggregate windows (`SUM() OVER`) in this issue
- Do not change GROUP BY aggregate semantics

## Test plan

```bash
cargo test -p rusql-executor window
cargo test -p rusql-server window
node scripts/mysql-diff.mjs
```
