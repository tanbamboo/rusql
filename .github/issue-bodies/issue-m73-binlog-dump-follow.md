## Goal

Keep `COM_BINLOG_DUMP` open after the current file tail so a replica receives events written by later `COMMIT`s, matching MySQL’s blocking dump (flag 0) vs one-shot `BINLOG_DUMP_NON_BLOCK` (0x01).

## Background

Phase Q after M72. Dump already splits events and INSERT is `TABLE_MAP` + `WRITE_ROWS`, but `handle_binlog_dump` still sends the file snapshot and an OK packet, then returns. A replica that connects before later commits never sees them.

## Acceptance Criteria

- [ ] `COM_BINLOG_DUMP` with flags `0` streams existing events from the requested position and **does not** send OK; later committed events (INSERT row events and/or UPDATE/DELETE QUERY) are written as additional `0x00` + event packets
- [ ] `BINLOG_DUMP_NON_BLOCK` (flags `0x01`) keeps today’s one-shot behavior: current events then OK (M71 test path)
- [ ] A wire test: dump client A (flags 0) is waiting; client B `BEGIN`/`INSERT`/`COMMIT`; A receives the new `TABLE_MAP` + `WRITE_ROWS` without reconnecting; A then quits (no unbounded wait)
- [ ] Client disconnect ends follow; do not leak dump tasks
- [ ] Docs: CHANGELOG, release-notes, user-guide (en+zh-CN), ADR, HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-server/src/**`
- `crates/rusql-storage/src/**` (only if a notify/subscribe helper is required)
- `crates/rusql-protocol/src/**` (dump flags / test helpers)
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- UPDATE_ROWS / DELETE_ROWS
- Semi-sync, GTID event type 33, group replication
- Heartbeat events, `COM_BINLOG_DUMP_GTID`
- New crates
- Changing WAL JSON format

## Negative Constraints

- Do not hang CI: follow tests must finish via a second-connection COMMIT plus client quit or a short timeout, never an infinite sleep
- Do not change row-event encode/decode layout from M72
- Do not require mysqlbinlog as a CI oracle

## Test plan

```bash
cargo test -p rusql-server binlog_dump
cargo test -p rusql-storage binlog
```
