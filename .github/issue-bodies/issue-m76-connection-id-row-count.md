## Goal

Expose MySQL `CONNECTION_ID()` and `ROW_COUNT()` so clients, ORMs, and admin tools can read the session thread id and the previous DML affected-row count.

## Background

Phase Q after M75. `LAST_INSERT_ID()` is session-scoped. `CONNECTION_ID()` is still `unsupported function`, and `ROW_COUNT()` is missing even though INSERT/UPDATE/DELETE already compute `rows_affected`. Handshake already assigns a per-connection id (`Session.id`).

## Acceptance Criteria

- [ ] `SELECT CONNECTION_ID()` returns the connection’s handshake thread id (same value as `SHOW PROCESSLIST` `Id` for that session)
- [ ] Two connections see different `CONNECTION_ID()` values
- [ ] `SELECT ROW_COUNT()` after `INSERT`/`UPDATE`/`DELETE` returns that statement’s affected-row count
- [ ] `SELECT ROW_COUNT()` after a `SELECT` returns `-1` (MySQL behavior)
- [ ] A second connection does not see the first connection’s `ROW_COUNT()`
- [ ] Unit/wire tests; `mysql-diff` suite where portable; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (function parse if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-core/src/**` (session field if needed)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout (M72–M74)
- `SQL_CALC_FOUND_ROWS` / `FOUND_ROWS()`
- `GET_LOCK` / `RELEASE_LOCK`
- New crates

## Negative Constraints

- Do not implement `FOUND_ROWS()` or `SQL_CALC_FOUND_ROWS` in this issue
- Do not change handshake connection-id allocation beyond exposing the existing session id
- Do not change DML affected-row accounting from current INSERT/UPDATE/DELETE semantics

## Test plan

```bash
cargo test -p rusql-executor connection_id
cargo test -p rusql-executor row_count
cargo test -p rusql-server connection_id
cargo test -p rusql-server row_count
```
