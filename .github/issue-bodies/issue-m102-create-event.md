## Goal

Persist a one-line `CREATE EVENT` catalog so `SHOW EVENTS` lists the event and `SHOW CREATE EVENT` reconstructs DDL instead of always returning an empty list / errno 1539.

## Background

Phase Q after M101. M100 accepts `SHOW CREATE EVENT` with missing-event errno 1539; M101 returns MySQL-shaped `SHOW EVENTS` columns over an empty catalog. sqlparser 0.53 does not expose `CREATE EVENT`; parse it in `try_parse_stored_program` (same path as `CREATE FUNCTION`). phpMyAdmin and dump tools send `CREATE EVENT … ON SCHEDULE … DO …` then list/reconstruct. This slice stores `EventMeta` in `ProgramStore` (`programs.json` `events` map with `serde(default)`) and the session catalog. The event scheduler does **not** run `DO` bodies. `ALTER EVENT`, live Definer / ON COMPLETION / charset persistence, and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `CREATE EVENT name ON SCHEDULE AT 'timestamp' DO stmt` and `CREATE EVENT name ON SCHEDULE EVERY n {SECOND|MINUTE|HOUR|DAY|WEEK|MONTH|YEAR} DO stmt` are parsed and persisted (optional `schema.name` / `IF NOT EXISTS` if a one-line parse is natural)
- [ ] Duplicate name is an error (i18n `event_exists`; MySQL-like errno 1537 `ER_EVENT_ALREADY_EXISTS` if a one-line `ExecError::Mysql` path is natural)
- [ ] `SHOW EVENTS` / `LIKE` lists catalog rows (`Db`/`Name` from `EventMeta`; `Type` is `ONE TIME` or `RECURRING`; schedule cells from AT/EVERY; Definer/timezone/charset/Originator stay documented stubs). Unmatched `LIKE` is zero rows; unknown `FROM` db stays errno 1049
- [ ] `SHOW CREATE EVENT name` reconstructs documented catalog DDL (`CREATE EVENT \`name\` ON SCHEDULE … DO …`); unknown names stay errno 1539
- [ ] `DROP EVENT name` (optional `IF EXISTS`) removes the catalog row
- [ ] `SHOW CREATE USER` from M99 and `SHOW FUNCTION STATUS` from M98 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (Definer/charset stubs differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-core/src/**` (`EventMeta`, catalog, `ProgramStore` events map with `serde(default)`)
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (`CREATE EVENT` / `DROP EVENT` parse)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (`event_exists` and related errors)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- Running the event scheduler / executing `DO` on a timer
- `ALTER EVENT`
- Persisting live DEFINER / ON COMPLETION / COMMENT / ENABLE ON SLAVE
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 event-scheduler parity; document catalog persistence without timed execution
- Do not change unknown-event `SHOW CREATE EVENT` errno 1539
- Do not implement `ALTER EVENT` or heartbeat / GTID event 33
- Do not invent last-executed timestamps

## Test plan

```bash
cargo test -p rusql-sql create_event
cargo test -p rusql-executor create_event
cargo test -p rusql-executor show_events
cargo test -p rusql-executor show_create_event
cargo test -p rusql-server create_event
```
