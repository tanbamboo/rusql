# ADR: Replication architecture (deferred)

## Status

Accepted — **not implemented**; documents direction for M10+ and closes planning gap (#5).

## Context

rusql targets MySQL 8.0 compatibility. Replication is out of scope for M0–M9 but must not be designed ad hoc when storage and protocol mature.

## Decision

1. **Defer implementation** until WAL + transactions are stable (post-M9).
2. **Primary/Replica model** aligned with MySQL async replication semantics (binlog-like stream), not synchronous cluster consensus in v1.
3. **WAL as replication source**: extend `rusql.wal` JSON lines (or successor binary log) as the canonical change stream; replicas replay the same `WalRecord` schema.
4. **Wire protocol**: replicas use standard client connection for read; replication channel uses a dedicated rusql extension command or sidecar gRPC (TBD in M10 spec) — **not** fake MySQL binlog protocol in v1.

## Consequences

- M9 transactions must commit atomically to WAL before visibility to other connections (feeds future replica lag model).
- Issue #5 satisfied by this ADR; implementation tracked as future `priority:P2` milestone.

## Alternatives considered

| Option | Rejected because |
|--------|------------------|
| Full MySQL binlog compatibility | Too large for MVP; poor fit with JSON WAL |
| Raft embedded in storage | Over-engineered before single-node stability |
