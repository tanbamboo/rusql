## Goal

MySQL binlog format for replication (see adr-replication.md).

## Depends on

- M31 durable WAL, ADR #5

## Acceptance Criteria

- [ ] ADR updated with binlog event subset
- [ ] Spike: write binlog header + QUERY_EVENT

## File Boundaries

- docs/en/specs/**, crates/rusql-storage/** (spike)
