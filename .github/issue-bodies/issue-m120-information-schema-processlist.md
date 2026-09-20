## Goal

Expose `information_schema.PROCESSLIST` as a queryable table backed by the existing M53 connection registry.

## Background

`SHOW PROCESSLIST` / `COM_PROCESS_INFO` exist. Probe: `information_schema_processlist` is missing (1146). Columns can match the SHOW list (`ID`, `USER`, `HOST`, `DB`, `COMMAND`, `TIME`, `STATE`, `INFO`) with MySQL information_schema names.

## Acceptance Criteria

- [ ] `SELECT ID FROM information_schema.PROCESSLIST` returns at least the current connection id (same as `CONNECTION_ID()`)
- [ ] Not errno 1146. `SHOW PROCESSLIST` columns/behavior unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` if Time/Info differ; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**` (`info_schema.rs`)
- `crates/rusql-core/src/processlist.rs` only if column naming requires it
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Full `performance_schema`
- WAL format
- Changing M53 SHOW column order

## Negative Constraints

- Do not invent extra processlist columns MySQL 8 dropped/added without tests

## Test plan

```bash
cargo test -p rusql-executor processlist
cargo test -p rusql-server processlist
```
