## Goal

Accept `SHOW BINARY LOGS` so replication ops probes are not an unsupported-statement error.

## Background

Phase R. Probe: `show_binary_logs`. rusql has binlog files from M56–M74. This slice lists known binlog files (at least the current file) with MySQL-shaped columns (`Log_name`, `File_size`). Sizes may be stubs if not cheap.

## Acceptance Criteria

- [ ] `SHOW BINARY LOGS` returns MySQL-shaped columns and at least one row when binlog is enabled (or a documented empty list if binlog file is missing)
- [ ] Not a parse error. `SHOW BINLOG EVENTS` stays later (M123)
- [ ] Unit/wire tests; `mysql-diff` `compare_output: false`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**` (rewrite if sqlparser lacks SHOW BINARY LOGS)
- `crates/rusql-executor/src/**`
- `crates/rusql-storage/src/**` (read binlog file names/sizes only)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- GTID event 33 / heartbeat (M173/M174)
- Changing binlog event layout
- `PURGE BINARY LOGS`

## Negative Constraints

- Do not claim mysqlbinlog tool compatibility

## Test plan

```bash
cargo test -p rusql-sql binary_log
cargo test -p rusql-executor binary_log
cargo test -p rusql-server binary_log
```
