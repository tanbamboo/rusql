## Goal

Accept `SHOW BINLOG EVENTS` (optional `LIMIT`) so operators can inspect the dump stream without an unsupported-statement error.

## Background

Depends on binlog files (M56+) and preferably M122. Probe: `show_binlog_events`. Documented subset of columns (`Log_name`, `Pos`, `Event_type`, `Server_id`, `End_log_pos`, `Info`).

## Acceptance Criteria

- [ ] `SHOW BINLOG EVENTS LIMIT 1` returns 0 or 1 row with MySQL-shaped columns (not a parse error)
- [ ] Unknown event types shown as documented names/numbers; do not crash
- [ ] `SHOW BINARY LOGS` unchanged if already landed
- [ ] Unit/wire tests; `mysql-diff` `compare_output: false`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/**`
- `crates/rusql-storage/src/**` (read-only binlog parse)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- GTID 33 / heartbeat
- Changing event write path
- `mysqlbinlog` CLI

## Negative Constraints

- Do not pretty-print every event type; unknown → documented stub Info cell

## Test plan

```bash
cargo test -p rusql-executor binlog_events
cargo test -p rusql-server binlog_events
```
