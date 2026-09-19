## Goal

Execute ENABLED one-time `CREATE EVENT … ON SCHEDULE AT` bodies when the timestamp is due, so `DO` DML is visible instead of remaining catalog-only.

## Background

Phase Q after M103. Events persist in `EventMeta`; `ALTER EVENT` can change schedule / status / name / body; `SHOW EVENTS` / `SHOW CREATE EVENT` list and reconstruct. The scheduler still never runs `DO`. phpMyAdmin and dump tools that create `AT` events expect the body to run. Recurring `EVERY`, last-executed timestamps, ON COMPLETION PRESERVE, DEFINER, a background timer thread, and `SET GLOBAL event_scheduler` stay later. This slice runs due `AT` events on the COM_QUERY path (deterministic; no sleep) and advertises `@@event_scheduler` as a read-only `ON` stub.

## Acceptance Criteria

- [ ] `SELECT @@event_scheduler` and `SHOW VARIABLES LIKE 'event_scheduler'` return `ON`; `SET @@event_scheduler` is errno 1238 (read-only, like `version`); `SET GLOBAL event_scheduler` stays errno 1229
- [ ] ENABLED `ONE TIME` events whose `execute_at` (stored `'YYYY-MM-DD HH:MM:SS'`) is `<=` current UTC timestamp have `DO` executed once on the triggering connection (same parse/plan/execute path as `CALL` body)
- [ ] After a successful run, the event is dropped (MySQL default `ON COMPLETION NOT PRESERVE`). `SHOW EVENTS` no longer lists it
- [ ] DISABLED events, future `AT` timestamps, and `RECURRING` / `EVERY` events are not executed. A `DO` error is logged and does not fail the client statement; the catalog row stays
- [ ] `ALTER EVENT` from M103 and `SHOW CREATE USER` from M99 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (Docker MySQL `event_scheduler` may be OFF / racing); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-core/src/**` (only if `EventMeta` helpers are required)
- `crates/rusql-executor/src/**` (`run_due_events`, `@@event_scheduler` stub)
- `crates/rusql-sql/src/**` (only if parse helpers are required)
- `crates/rusql-server/src/**` (invoke scheduler on COM_QUERY)
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- Background tokio timer / sleep-based tests
- Recurring `EVERY` interval ticking
- Persisting live DEFINER / ON COMPLETION / COMMENT / last-executed timestamps
- `SET GLOBAL event_scheduler`
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 event-scheduler parity; document one-time due `AT` execution without a timer thread
- Do not change `ALTER EVENT` catalog behavior from M103
- Do not invent last-executed timestamps on `SHOW EVENTS`
- Do not implement GTID event type 33 / heartbeat

## Test plan

```bash
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-executor session_var
cargo test -p rusql-server event_scheduler
```
