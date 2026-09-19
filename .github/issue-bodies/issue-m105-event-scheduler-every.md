## Goal

Execute ENABLED recurring `CREATE EVENT … ON SCHEDULE EVERY n UNIT` bodies when the interval has elapsed, so `DO` DML repeats instead of remaining catalog-only after M104’s one-time `AT` path.

## Background

Phase Q after M104. Due `ONE TIME` `AT` events run `DO` and are dropped. Recurring `EVERY` events still never run. phpMyAdmin and dump tools that schedule `EVERY 1 HOUR` expect the body to execute. This slice stores `last_executed` on `EventMeta` (`serde(default)`; **not** a `SHOW EVENTS` column) and reuses the COM_QUERY `run_due_events` hook (deterministic `now`; no sleep / timer thread). `STARTS` / `ENDS`, last-executed on `SHOW EVENTS`, DEFINER, ON COMPLETION, and GTID event 33 stay later.

Never-executed ENABLED `EVERY` events are due on the next COM_QUERY (testable without sleep). After a successful run, the next due time is `last_executed + interval`. `MONTH` / `YEAR` use 30-day / 365-day approximations.

## Acceptance Criteria

- [ ] ENABLED `RECURRING` events (`EVERY n {SECOND|MINUTE|HOUR|DAY|WEEK|MONTH|YEAR}`) run `DO` when `last_executed` is unset, or when `last_executed + interval` is `<=` current UTC timestamp
- [ ] After a successful run the catalog row stays; `last_executed` is set to `now`. The event is not dropped. `SHOW EVENTS` still lists it and does **not** gain a last-executed column
- [ ] DISABLED `EVERY` events are not run. `DO` errors leave the row and do not fail the client statement
- [ ] M104 one-time due `AT` (run then drop) and `SHOW CREATE USER` from M99 are unchanged. `programs.json` without `last_executed` still loads
- [ ] Unit/wire tests with injected `now` (no sleep); `mysql-diff` with `compare_output: false`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-core/src/**` (`EventMeta.last_executed` with `serde(default)`)
- `crates/rusql-executor/src/**` (`run_due_events` interval due-check)
- `crates/rusql-sql/src/**` (only if `EventMeta` construction requires it)
- `crates/rusql-server/src/**` (wire tests / invoke path only if required)
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- Background tokio timer / sleep-based tests
- Adding last-executed to `SHOW EVENTS` output
- `STARTS` / `ENDS` / ON COMPLETION / DEFINER persistence
- `SET GLOBAL event_scheduler`
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 event-scheduler parity; document first-fire-on-next-COM_QUERY plus interval, without a timer thread
- Do not change M104 one-time `AT` drop-after-run behavior
- Do not invent last-executed cells on `SHOW EVENTS`
- Do not implement GTID event type 33 / heartbeat

## Test plan

```bash
cargo test -p rusql-core programs
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-server event_scheduler
```
