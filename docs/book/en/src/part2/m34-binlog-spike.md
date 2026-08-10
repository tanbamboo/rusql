# M34 — Binlog replication spike

**Issue #57**

## Problem

External replication tools speak MySQL **binlog**, not JSON WAL. We needed a research spike to see whether exporting changes as binlog events is feasible without replacing `rusql.wal`.

## Decision

- Keep JSON WAL as the durability source of truth.
- Add `binlog.rs` spike: magic header + `FORMAT_DESCRIPTION_EVENT` + `QUERY_EVENT`.
- Update [adr-replication.md](../../../en/specs/adr-replication.md) with the M34 event subset and explicit non-goals (GTID, row events, checksums).

## Trade-offs

The spike writes files but is not wired to the server loop. Full replication remains deferred; the ADR documents the path.

## Harness lesson

> **Spike milestones** belong in storage with unit tests and an ADR update — not half-integrated into production paths.
