## Goal

Accept text-protocol `PREPARE stmt FROM 'sql'` / `EXECUTE stmt` / `DEALLOCATE PREPARE stmt` so clients that do not use `COM_STMT_*` can run prepared SQL.

## Background

M11 binary prepared statements exist. Probe: `prepare_execute_text`. Session-scoped named statements; `COM_RESET_CONNECTION` clears them.

## Acceptance Criteria

- [ ] `PREPARE gap_stmt FROM 'SELECT 1'` then `EXECUTE gap_stmt` returns the same result as `SELECT 1`
- [ ] `DEALLOCATE PREPARE gap_stmt` then `EXECUTE gap_stmt` is an error (MySQL errno 1243 or documented)
- [ ] `PREPARE` of unsupported SQL fails at prepare or execute consistently (document)
- [ ] Binary `COM_STMT_*` unchanged
- [ ] Unit/wire tests; `mysql-diff`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/**`
- `crates/rusql-core/src/**` (session map of named statements)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- User variables in PREPARE beyond `@foo` already in session
- WAL format
- New crates

## Negative Constraints

- Do not implement `EXECUTE … USING` unless tests pin a one-arg form
- Do not change binary protocol statement ids

## Test plan

```bash
cargo test -p rusql-executor prepare
cargo test -p rusql-server prepare
```
