# ADR: Replication architecture (deferred)

## Status

Accepted — **M34 spike landed** (binlog header + QUERY_EVENT writer); full replication not implemented.

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

**Not in spike**: `TABLE_MAP_EVENT`, row events, full GTID SET negotiation, checksum verification, semi-sync.

## P3 MVP (programs + binlog on COMMIT)

Server command loop now:

1. Parses stored-program DDL/DML via `try_parse_stored_program`.
2. Appends WAL records to binlog QUERY events on `COMMIT` (`BinlogWriter::append_commit`).
3. Handles `COM_BINLOG_DUMP` / `COM_REGISTER_SLAVE` and `SHOW MASTER/SLAVE STATUS` stubs.

Replica helper: `apply_binlog_file(path, |schema, sql| { … })` replays QUERY events.

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
