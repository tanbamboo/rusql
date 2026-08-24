## Goal

Assign and track Global Transaction Identifiers for replication failover.

## Category

Phase N — Replication.

## Depends on

- M57 replica applier

## Acceptance Criteria

- [ ] `gtid_mode=ON` server flag (documented MVP)
- [ ] GTID included in binlog events for committed transactions
- [ ] Replica skips already-applied GTID set
- [ ] `SHOW MASTER STATUS` / `SHOW SLAVE STATUS` GTID fields stub
- [ ] ADR documents failover limitations

## File Boundaries

- `crates/rusql-storage/**`, `crates/rusql-server/**`, `docs/en/specs/**`

## Negative Constraints

- No automatic leader election
- No MySQL Group Replication protocol
