## Goal

Expose catalog events through `information_schema.EVENTS` so GUIs that read `EVENT_COMMENT` / `LAST_EXECUTED` / `DEFINER` / `ON_COMPLETION` get MySQL-shaped rows instead of errno 1146.

## Background

Phase Q after M108. `SHOW EVENTS` is 15 columns and must stay that way (MySQL has no last-executed there). phpMyAdmin and dump tools query `information_schema.EVENTS`. Probe (`scripts/mysql-gap-probe.mjs`, 2026-09-20): `information_schema_events` → errno 1146 `Table 'information_schema.EVENTS' doesn't exist`. Internal `EventMeta.last_executed` already exists (M105 watermark) and can fill `LAST_EXECUTED`.

Depends on M107 (DEFINER / ON COMPLETION) and M108 (COMMENT) for those cells; if M108 is not merged yet, `EVENT_COMMENT` may be empty.

## Acceptance Criteria

- [ ] `SELECT EVENT_NAME FROM information_schema.EVENTS` (and `information_schema.events`) returns one row per catalog event, not errno 1146
- [ ] Documented MySQL-like columns including at least `EVENT_SCHEMA`, `EVENT_NAME`, `DEFINER`, `EVENT_TYPE`, `EXECUTE_AT`, `INTERVAL_VALUE`, `INTERVAL_FIELD`, `STARTS`, `ENDS`, `STATUS`, `ON_COMPLETION`, `LAST_EXECUTED`, `EVENT_COMMENT`
- [ ] `LAST_EXECUTED` comes from `EventMeta.last_executed` (empty when unset). `EVENT_COMMENT` from `EventMeta.comment` (empty when unset)
- [ ] `SHOW EVENTS` still has 15 columns. M107 DEFINER / ON COMPLETION reconstruction is unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**` (`info_schema.rs` and query dispatch)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Adding columns to `SHOW EVENTS`
- Background timer / `SET GLOBAL event_scheduler`
- Full MySQL 8.0 `information_schema` catalog
- New crates
- `information_schema.PARAMETERS` / `TABLE_CONSTRAINTS` / `PROCESSLIST` (later)

## Negative Constraints

- Do not add last-executed to `SHOW EVENTS`
- Do not invent timestamps that were never stored
- Do not implement GTID event type 33 / heartbeat

## Test plan

```bash
cargo test -p rusql-executor information_schema
cargo test -p rusql-executor events
cargo test -p rusql-server information_schema
```
