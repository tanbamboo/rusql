## Goal

Accept `ALTER EVENT` so clients can change catalog schedule / status / body / name instead of getting a parse or unsupported-statement error.

## Background

Phase Q after M102. `CREATE EVENT` persists `EventMeta`; `SHOW EVENTS` lists it and `SHOW CREATE EVENT` reconstructs DDL. sqlparser 0.53 does not expose `ALTER EVENT`; parse it in `try_parse_stored_program` (same path as `CREATE EVENT`). phpMyAdmin and dump tools send `ALTER EVENT … ON SCHEDULE …` / `ENABLE` / `DISABLE` / `RENAME TO` / `DO`. This slice updates the existing catalog row. Timed execution of `DO`, DEFINER / ON COMPLETION / COMMENT persistence, and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `ALTER EVENT name ON SCHEDULE AT 'timestamp'` and `ALTER EVENT name ON SCHEDULE EVERY n {SECOND|MINUTE|HOUR|DAY|WEEK|MONTH|YEAR}` update the catalog schedule (optional `schema.name` if a one-line parse is natural)
- [ ] `ALTER EVENT name ENABLE` / `DISABLE`, optional `RENAME TO new_name`, and optional `DO stmt` update status / name / body when present
- [ ] Unknown event is errno 1539 (`ER_EVENT_DOES_NOT_EXIST`)
- [ ] `SHOW EVENTS` and `SHOW CREATE EVENT` reflect the updated catalog row
- [ ] `SHOW CREATE USER` from M99 and `SHOW FUNCTION STATUS` from M98 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (Definer/charset stubs differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-core/src/**` (`EventMeta` / catalog update helpers)
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (`ALTER EVENT` parse)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- Running the event scheduler / executing `DO` on a timer
- Persisting live DEFINER / ON COMPLETION / COMMENT / ENABLE ON SLAVE
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `ALTER EVENT` parity; document catalog updates without timed execution
- Do not change unknown-event `SHOW CREATE EVENT` errno 1539
- Do not implement event scheduler execution or GTID event 33
- Do not invent last-executed timestamps

## Test plan

```bash
cargo test -p rusql-sql alter_event
cargo test -p rusql-executor alter_event
cargo test -p rusql-server alter_event
```
