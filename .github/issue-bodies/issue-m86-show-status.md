## Goal

Implement `SHOW STATUS` / `SHOW SESSION STATUS` / `SHOW GLOBAL STATUS` so clients and monitors that probe a small documented stub catalog get `Variable_name`/`Value` rows instead of an unsupported-statement error.

## Background

Phase Q after M85. sqlparser already parses `Statement::ShowStatus` (including `LIKE` / `WHERE`). rusql ignores it today. Official CLI, JDBC, and health checks send `SHOW STATUS` or `SHOW STATUS LIKE 'Threads_connected'`. This slice is a documented stub catalog only — not live InnoDB/performance_schema counters. GTID event type 33 / heartbeat stays later.

## Acceptance Criteria

- [ ] `SHOW STATUS` returns columns `Variable_name`, `Value` with one row per documented stub (at least: `Uptime`, `Threads_connected`, `Threads_running`, `Questions`, `Slow_queries`, `Open_tables`, `Connections`, `Aborted_connects`, `Bytes_received`, `Bytes_sent`)
- [ ] `SHOW SESSION STATUS` is equivalent to `SHOW STATUS` for this set
- [ ] `SHOW GLOBAL STATUS` returns the same stub rows (document that session=global for this slice)
- [ ] `SHOW STATUS LIKE 'Threads%'` (MySQL `LIKE` pattern) filters that set; a non-matching pattern returns zero rows, not an error
- [ ] Stub values are stable documented constants (not live engine counters), except `Threads_connected` MAY equal the current connection count if that is already available without new infrastructure
- [ ] `SELECT … FOR UPDATE` from M85 and `SET TRANSACTION ISOLATION LEVEL` from M84 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (row counts and values differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (parse `SHOW [SESSION|GLOBAL] STATUS [LIKE …]` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-core/src/**` (only if connection-count for `Threads_connected` needs a session/server field already used by `SHOW PROCESSLIST`)
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- Full MySQL 8.0 status catalog / `performance_schema`
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- `FLUSH STATUS`
- Live query/byte counters unless a one-line existing field
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW STATUS` parity; document the stub subset
- Do not implement `WHERE` filtering unless sqlparser already exposes it as a one-line wire-up (`LIKE` is required)
- Do not implement `SHOW STATUS LIKE` as an error for unknown names; unmatched `LIKE` is zero rows

## Test plan

```bash
cargo test -p rusql-executor show_status
cargo test -p rusql-server show_status
```
