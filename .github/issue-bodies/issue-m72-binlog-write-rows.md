## Goal

Emit MySQL `TABLE_MAP_EVENT` + `WRITE_ROWS_EVENT` for committed `INSERT`s so a replica can apply row images instead of only QUERY text.

## Background

Phase Q after M71. Dump now streams per-event packets, but the payload is still QUERY_EVENT SQL. Row events are the Phase N exit criterion (primary → replica row-level consistency for a DML subset). This slice covers **INSERT only**.

## Acceptance Criteria

- [ ] A committed `INSERT INTO t VALUES (…)` writes `TABLE_MAP` then `WRITE_ROWS` (v1 or v2 documented) after `FORMAT_DESCRIPTION`
- [ ] `extract`/`apply` path can reconstruct the insert on a second engine (table must already exist on replica, or QUERY DDL remains out of scope)
- [ ] `COM_BINLOG_DUMP` from position 4 includes the new event types as separate packets (reuse M71 splitter)
- [ ] Existing QUERY_EVENT path for UPDATE/DELETE is unchanged
- [ ] Unit tests (event encode/decode) + server dump test; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), ADR, HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-storage/src/**`
- `crates/rusql-server/src/**` (tests / dump only if needed)
- `crates/rusql-protocol/src/**` (only if dump packet tests need helpers)
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- UPDATE_ROWS / DELETE_ROWS in this issue
- Semi-sync, GTID event type 33, group replication
- Live blocking dump follow
- New crates
- Changing WAL JSON format

## Negative Constraints

- Do not remove QUERY_EVENT for UPDATE/DELETE
- Do not require mysqlbinlog as a CI oracle (optional local check is fine)
- Do not implement partial row images / JSON binary row encoding beyond current string cells

## Test plan

```bash
cargo test -p rusql-storage binlog
cargo test -p rusql-server binlog_dump
```
