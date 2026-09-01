## Goal

Extend M34 binlog spike into a durable, ordered event stream suitable for replication.

## Category

Phase N — Replication.

## Depends on

- M34 binlog QUERY_EVENT spike, M31 durable WAL

## Acceptance Criteria

- [ ] Binlog file rotation with magic + Format_description_event
- [ ] QUERY_EVENT + Table_map + Write_rows / Update_rows / Delete_rows for DML subset
- [ ] Checksum setting documented (OFF or CRC32)
- [ ] `mysqlbinlog` can parse rusql output for supported events
- [ ] ADR updated in `docs/en/specs/adr-replication.md`

## File Boundaries

- `crates/rusql-storage/**`, `docs/en/specs/**`

## Negative Constraints

- No GTID events until M58
- No group replication
