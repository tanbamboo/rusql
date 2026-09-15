## Goal

Expose MySQL `FOUND_ROWS()` and accept `SELECT SQL_CALC_FOUND_ROWS … LIMIT` so clients that paginate with the deprecated calc-found-rows pattern get a row count instead of `unsupported function`.

## Background

Phase Q after M77. `ROW_COUNT()` is `-1` after a result-set `SELECT`. Some connectors and older ORMs still call `FOUND_ROWS()` after `SELECT SQL_CALC_FOUND_ROWS … LIMIT n` to learn the un-LIMITed match count. MySQL 8.0.17 deprecated `SQL_CALC_FOUND_ROWS`; this slice is a compatibility stub, not a replacement for `COUNT(*)`.

## Acceptance Criteria

- [ ] `SELECT FOUND_ROWS()` after a plain `SELECT` returns the number of rows in that result set (after `LIMIT`)
- [ ] `SELECT SQL_CALC_FOUND_ROWS … LIMIT n` then `SELECT FOUND_ROWS()` returns the matching row count **without** applying `LIMIT`
- [ ] After `INSERT`/`UPDATE`/`DELETE`, `FOUND_ROWS()` returns the same affected-row count as `ROW_COUNT()`
- [ ] A second connection does not see the first connection’s `FOUND_ROWS()`
- [ ] `COM_RESET_CONNECTION` / `COM_CHANGE_USER` reset `FOUND_ROWS()` to `0`
- [ ] Unit/wire tests; `mysql-diff` where portable (`compare_output: false` if Docker MySQL differs); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (parse `SQL_CALC_FOUND_ROWS` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-core/src/**` (session field if needed)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- Replacing `LIMIT` pagination with window/`COUNT(*)` rewrite as a general planner
- New `@@` variables / `SHOW VARIABLES`
- New crates

## Negative Constraints

- Do not implement `SQL_CACHE` / `SQL_NO_CACHE` or other `SELECT` modifiers
- Do not change `ROW_COUNT()` SELECT = `-1` semantics
- Do not claim `SQL_CALC_FOUND_ROWS` is the recommended MySQL 8.0 pagination API (`COUNT(*)` remains the supported alternative)

## Test plan

```bash
cargo test -p rusql-executor found_rows
cargo test -p rusql-server found_rows
```
