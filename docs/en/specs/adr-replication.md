# ADR: Replication architecture (deferred)

## Status

Accepted — M34 spike, P3 MVP QUERY stream, M71 per-event dump, **M72 INSERT row events**. Full MySQL replication is not implemented.

## Context

rusql targets MySQL 8.0 compatibility. Replication is out of scope for M0–M9 but must not be designed ad hoc when storage and protocol mature.

## Decision

1. **Defer full replication** until WAL + transactions are stable (post-M9, M31 done).
2. **Primary/Replica model** aligned with MySQL async replication semantics (binlog-like stream), not synchronous cluster consensus in v1.
3. **WAL as canonical change stream**: `rusql.wal` JSON lines remain the source of truth for durability; binlog is a **derived export** format for external consumers.
4. **Wire protocol**: replicas use standard client connection for read; replication channel uses a dedicated rusql extension command or sidecar gRPC (TBD) — **not** fake full MySQL replication handshake in v1.

## M34 binlog event subset (spike)

Implemented in `crates/rusql-storage/src/binlog.rs` as research code:

| Event | Type byte | Purpose |
|-------|-----------|---------|
| `FORMAT_DESCRIPTION_EVENT` | 15 | File header after magic; documents binlog v4 + server version |
| `QUERY_EVENT` | 2 | DDL/DML as SQL text + default schema |

File layout:

1. Magic `0xfe 0x62 0x69 0x6e` (`þbin`)
2. `FORMAT_DESCRIPTION_EVENT` (required first event)
3. `QUERY_EVENT` records appended by `write_binlog_spike()`

**Not in spike**: full GTID SET negotiation, checksum verification, semi-sync.

## P3 MVP (programs + binlog on COMMIT)

Server command loop now:

1. Parses stored-program DDL/DML via `try_parse_stored_program`.
2. Appends WAL records to binlog on `COMMIT` (`BinlogWriter::append_commit`).
3. Handles `COM_BINLOG_DUMP` as **per-event** packets (`0x00` + event, from requested position). Flags `0` keep the connection open and stream later COMMITs; `BINLOG_DUMP_NON_BLOCK` (`0x01`) is one-shot then OK. Also `COM_REGISTER_SLAVE` and `SHOW MASTER/SLAVE STATUS` stubs.

Replica helper: `apply_binlog_file(path, |schema, sql| { … })` replays QUERY events and INSERT SQL reconstructed from row events.

## M72 INSERT row events

Committed `INSERT` WAL records write:

| Event | Type byte | Purpose |
|-------|-----------|---------|
| `TABLE_MAP_EVENT` | 19 | schema + table + column count; types are `MYSQL_TYPE_VARCHAR` (0x0f) |
| `WRITE_ROWS_EVENT_V1` | 23 | one row image matching that table_id |

Layout after the 19-byte common header (rusql-internal; **not** a mysqlbinlog oracle):

1. `table_id` u64 little-endian (hash of schema + table name)
2. `flags` u16 (always 0)
3. TABLE_MAP: schema_len + schema + NUL + table_len + table + NUL + col_count + type bytes
4. WRITE_ROWS: width u8, included-column bitmap, null bitmap, then non-null cells as `u32` length + UTF-8

Empty cells set the null bit and reconstruct as SQL `NULL`. GTID still advances once per commit (`gtid.json`) even when the commit is INSERT-only. DDL is not written; the replica table must already exist.

**Not in M72**: `UPDATE_ROWS` / `DELETE_ROWS`, GTID event type 33, checksum bytes.

## M73 live dump follow

`COM_BINLOG_DUMP` flag `0` (MySQL default) streams the current file from the requested position and **does not** send OK. Dump waiters share a `watch` generation bumped after `append_commit`, so a replica that connected before a later `COMMIT` receives new `0x00` + event packets on the same connection. `BINLOG_DUMP_NON_BLOCK` (`0x01`) keeps the M71 one-shot + OK behavior. `COM_QUIT` or TCP disconnect ends follow (no extra dump tasks). Heartbeat events and `COM_BINLOG_DUMP_GTID` remain out of scope.

**Not in M73**: `UPDATE_ROWS` / `DELETE_ROWS`, semi-sync, GTID event type 33, heartbeat.

## M74 UPDATE/DELETE row events

Committed `UPDATE` WAL records write `TABLE_MAP` then `UPDATE_ROWS_EVENT_V1` (type 24). Committed `DELETE` writes `TABLE_MAP` then `DELETE_ROWS_EVENT_V1` (type 25). Layout after the 19-byte header (rusql-internal):

1. `table_id` u64 little-endian (same hash as M72)
2. `flags` u16 (always 0)
3. UPDATE: `nset` u8, then `nset` times (column u32-len + UTF-8, value u32-len + UTF-8), then `has_where` u8 and optional WHERE column/value
4. DELETE: `has_where` u8 and optional WHERE column/value

`extract`/`apply_binlog_file` reconstruct `UPDATE … SET` / `DELETE FROM` using `sql_literal` (empty value → SQL `NULL`). WAL JSON is unchanged. Remaining QUERY_EVENT is for other WAL ops (DDL is still not in the binlog).

**Not in M74**: full before/after row images, GTID event type 33, checksum bytes, heartbeat.

## Consequences

- M9/M31 transactions commit atomically to WAL before cross-connection visibility (feeds future replica lag model).
- Binlog spike validates event encoding without coupling to the server command loop.
- Issue #5 satisfied by this ADR; M34 closes the planning spike (#57).

## Alternatives considered

| Option | Rejected because |
|--------|------------------|
| Full MySQL binlog compatibility | Too large for MVP; poor fit with JSON WAL |
| Raft embedded in storage | Over-engineered before single-node stability |
| Replace WAL with binlog | Loses simple JSON replay and test ergonomics |
