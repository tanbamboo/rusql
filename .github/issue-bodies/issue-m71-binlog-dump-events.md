## Goal

Make `COM_BINLOG_DUMP` emit MySQL-style **per-event** dump packets from the requested file position, so a replica can consume the existing QUERY_EVENT stream without receiving the whole binlog as one blob.

## Background

Phase Q after M70. M56–M58 shipped an MVP: QUERY events on `COMMIT`, a one-shot dump of leftover file bytes, and `apply_binlog_file` replay. The dump path still concatenates raw file tail into a single packet (`handle_binlog_dump`), which is not how MySQL replica I/O reads events. This issue splits events and honors `position` without adding row-based logging.

## Acceptance Criteria

- [ ] `COM_BINLOG_DUMP` sends each binlog event as its own packet: `0x00` + event bytes (no wrapping magic into the first event)
- [ ] Requested `position` skips earlier bytes; dumping from 4 yields `FORMAT_DESCRIPTION` first
- [ ] After `BEGIN`/`INSERT`/`COMMIT` on the primary, a dump from the current file can be reassembled and `apply_binlog_file` (or equivalent) makes the INSERT visible on a second engine
- [ ] Dump of an empty/new file still includes `FORMAT_DESCRIPTION`
- [ ] Unit tests (event split) + server/wire test; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), roadmap Phase Q, HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-storage/src/**`
- `crates/rusql-protocol/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (only if a new client-visible error is required)
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- `TABLE_MAP_EVENT` / `WRITE_ROWS` / `UPDATE_ROWS` / `DELETE_ROWS`
- Semi-sync, GTID event type 33, group replication
- Multi-source replication / automatic failover
- New crates
- Changing WAL JSON format

## Negative Constraints

- Do not implement row-based binlog in this issue
- Do not keep the dump connection blocking forever waiting for new commits (one-shot dump of the current file is OK)
- Do not change `SHOW MASTER STATUS` / `SHOW SLAVE STATUS` column layout unless a test requires a documented field

## Test plan

```bash
cargo test -p rusql-storage binlog
cargo test -p rusql-server binlog_dump
```
