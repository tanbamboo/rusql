## Goal

Emit MySQL `UPDATE_ROWS` / `DELETE_ROWS` (v1) for committed UPDATE/DELETE so a replica can apply row images instead of QUERY text, matching M72 INSERT row events.

## Background

Phase Q after M73. INSERT already writes `TABLE_MAP` + `WRITE_ROWS`. UPDATE/DELETE still use `QUERY_EVENT` with a GTID comment. WAL records (`UpdateRows` / `DeleteRows`) already carry SET assignments and optional WHERE; do **not** change WAL JSON. Layout stays rusql-internal (UTF-8 cells with u32 length prefix, 8-byte table_id), not a mysqlbinlog oracle.

## Acceptance Criteria

- [ ] A committed `UPDATE t SET … WHERE …` writes `TABLE_MAP_EVENT` (type 19) then `UPDATE_ROWS_EVENT_V1` (type 24) instead of `QUERY_EVENT`
- [ ] A committed `DELETE FROM t WHERE …` writes `TABLE_MAP` then `DELETE_ROWS_EVENT_V1` (type 25)
- [ ] `extract` / `apply_binlog_file` reconstruct `UPDATE` / `DELETE` SQL (table must already exist on replica)
- [ ] INSERT `TABLE_MAP` + `WRITE_ROWS` and live dump follow (M72/M73) stay unchanged
- [ ] Cell encoding matches M72 (UTF-8, u32 length, empty cell = SQL `NULL`)
- [ ] Unit tests + server dump/follow coverage; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), ADR, HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-storage/src/**`
- `crates/rusql-server/src/**` (dump/follow tests if needed)
- `crates/rusql-protocol/src/**` (only if dump helpers need constants)
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Semi-sync, GTID event type 33, group replication
- Heartbeat, `COM_BINLOG_DUMP_GTID`
- Full before/after row images beyond what WAL already stores
- New crates

## Negative Constraints

- Do not change INSERT row-event layout from M72
- Do not require mysqlbinlog as a CI oracle
- Do not hang CI (reuse M73 follow timeouts if adding a follow assertion)

## Test plan

```bash
cargo test -p rusql-storage binlog
cargo test -p rusql-server binlog_dump
```
