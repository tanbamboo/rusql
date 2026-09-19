## Goal

Honor `STARTS` / `ENDS` on recurring `EVERY` events so the scheduler does not fire before the start timestamp or after the end timestamp.

## Background

Phase Q after M105. Recurring `EVERY` events fire on the next COM_QUERY then on `last_executed + interval`. phpMyAdmin and dump tools send `CREATE EVENT … ON SCHEDULE EVERY n UNIT STARTS '…' ENDS '…'`. `SHOW EVENTS` already has `Starts` / `Ends` columns as empty stubs. This slice parses and persists those timestamps (`EventMeta.starts` / `ends`, `serde(default)`), fills `SHOW EVENTS` / `SHOW CREATE EVENT`, and gates `run_due_events`. ON COMPLETION, DEFINER, last-executed on `SHOW EVENTS`, and a timer thread stay later.

## Acceptance Criteria

- [ ] `CREATE EVENT name ON SCHEDULE EVERY n UNIT STARTS 'ts'` and optional `ENDS 'ts'` persist the timestamps (quoted `YYYY-MM-DD HH:MM:SS`)
- [ ] `ALTER EVENT name ON SCHEDULE EVERY n UNIT STARTS 'ts' [ENDS 'ts']` and `ALTER EVENT name STARTS 'ts'` / `ENDS 'ts'` update the catalog
- [ ] ENABLED `EVERY` events are not due when `now < starts` or `now > ends`. First fire still happens on the next COM_QUERY once `starts` is due (injected `now`; no sleep)
- [ ] `SHOW EVENTS` `Starts` / `Ends` cells come from the catalog (empty when unset). `SHOW CREATE EVENT` reconstructs `STARTS` / `ENDS`. Column count stays 15
- [ ] M105 interval watermark, M104 one-time `AT` drop-after-run, and `SHOW CREATE USER` from M99 are unchanged. `programs.json` without `starts`/`ends` still loads
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-core/src/**` (`EventMeta.starts` / `ends` with `serde(default)`)
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (`STARTS` / `ENDS` parse)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- Background tokio timer / sleep-based tests
- Last-executed column on `SHOW EVENTS`
- ON COMPLETION / DEFINER / COMMENT persistence
- `SET GLOBAL event_scheduler`
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 event-scheduler parity; document STARTS/ENDS gates without a timer thread
- Do not change M104 `AT` drop-after-run or M105 interval watermark behavior
- Do not invent last-executed cells on `SHOW EVENTS`
- Do not implement GTID event type 33 / heartbeat

## Test plan

```bash
cargo test -p rusql-sql alter_event
cargo test -p rusql-sql create_event
cargo test -p rusql-core programs
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-server event_scheduler
```
