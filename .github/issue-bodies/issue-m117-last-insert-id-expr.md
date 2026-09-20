## Goal

Implement `LAST_INSERT_ID(expr)` setter so the session value (and subsequent no-arg `LAST_INSERT_ID()`) follows MySQL sequence-helper semantics.

## Background

M75 implemented no-arg `LAST_INSERT_ID()`. Probe: `fn_last_insert_id_expr`. MySQL: `SELECT LAST_INSERT_ID(5)` returns 5 and sets the session value used by later `LAST_INSERT_ID()`.

## Acceptance Criteria

- [ ] `SELECT LAST_INSERT_ID(5)` returns `5` and a following `SELECT LAST_INSERT_ID()` returns `5` on that connection
- [ ] Non-integer expr: document coercion (MySQL converts; pin tests)
- [ ] Other connections are unchanged. `COM_RESET_CONNECTION` clears (same as M75)
- [ ] No-arg `LAST_INSERT_ID()` and INSERT-generated ids from M75 still work
- [ ] Unit/wire tests; `mysql-diff`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**` (`expr.rs`, session updates)
- `crates/rusql-core/src/**` (session field only if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing AUTO_INCREMENT allocation (M37)
- WAL format
- `JSON_EXTRACT` / `UUID`

## Negative Constraints

- Do not change INSERT OK-packet last_insert_id except when MySQL would for this setter (document)

## Test plan

```bash
cargo test -p rusql-executor last_insert
cargo test -p rusql-server last_insert
```
