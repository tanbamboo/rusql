## Goal

Persist event `COMMENT` so dumps and `SHOW CREATE EVENT` reconstruct `COMMENT '…'`, matching phpMyAdmin/mysqldump shape.

## Background

Phase Q after M107. `CREATE EVENT … COMMENT '…' DO …` currently falls through to sqlparser (`Expected: an object type after CREATE, found: EVENT`). Client dumps send `COMMENT` after `ON COMPLETION` / `ENABLE`. `SHOW EVENTS` stays 15 columns (MySQL has no last-executed column there). `information_schema.EVENTS` is M109.

Probe evidence (`scripts/mysql-gap-probe.mjs`, 2026-09-20): `event_comment` → parse_error 1064.

## Acceptance Criteria

- [ ] `CREATE EVENT name ON SCHEDULE … [ON COMPLETION …] [ENABLE|DISABLE] COMMENT 'text' DO stmt` persists `EventMeta.comment` (`serde(default)`)
- [ ] `ALTER EVENT name COMMENT 'text'` updates the catalog
- [ ] `SHOW CREATE EVENT` reconstructs `COMMENT 'text'` when set. Empty/omitted comment is omitted from DDL (MySQL default)
- [ ] `SHOW EVENTS` column count stays 15 (no last-executed column)
- [ ] M107 DEFINER / ON COMPLETION, M106 `STARTS`/`ENDS`, and `SHOW CREATE USER` from M99 are unchanged. `programs.json` without `comment` still loads
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-core/src/**` (`EventMeta.comment` with `serde(default)`)
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (`COMMENT` parse on events only)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- Adding a last-executed column to `SHOW EVENTS`
- `information_schema.EVENTS` (M109)
- `DISABLE ON SLAVE` persistence
- Background tokio timer / sleep-based tests
- `SET GLOBAL event_scheduler`
- New crates
- COMMENT on procedures / functions / triggers / views

## Negative Constraints

- Do not claim full MySQL 8.0 event-scheduler parity
- Do not invent `SHOW EVENTS` columns
- Do not implement GTID event type 33 / heartbeat

## Test plan

```bash
cargo test -p rusql-sql create_event
cargo test -p rusql-sql alter_event
cargo test -p rusql-core programs
cargo test -p rusql-executor show_create_event
cargo test -p rusql-server create_event
```
