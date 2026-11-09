# ADR: M199 Group Replication membership subset

**Status**: Accepted (M199 / [#352](https://github.com/tanbamboo/rusql/issues/352))  
**Date**: 2026-11-09

## Context

MySQL 8.0 Group Replication (and InnoDB Cluster built on it) provides multi-primary or single-primary HA with a GCS/XCom membership layer, certification, and conflict handling. rusql already has async binlog dump/apply (M179), manual promote (M177), and persisted `@@gtid_executed` (M176). Building XCom from scratch is out of scope for this milestone and would not be a faithful Oracle wire peer. Leaving GR unmarked blocks M210. A silent skip is forbidden. This decision does not change `CONSTITUTION.md`.

## Decision

1. **Implement a Group Replication membership subset**, not an M177-only deferral and not a silent skip. Community `group_replication` stays in scope; this slice is the first queryable membership surface.

2. **No XCom from scratch.** There is no GCS protocol, no group communication messages, and no claim of Oracle Group Replication wire compatibility.

3. **Config-driven three-member membership.** Persist `{data_dir}/group_replication.json` with a group name, the local member id, and three member endpoints (or three in-process logical members seeded on first `START`). Membership is not discovered over the network in this slice.

4. **SQL control.** `START GROUP_REPLICATION` marks the three members `ONLINE` in single-primary mode (local member `PRIMARY`, the other two `SECONDARY`) and returns OK. `STOP GROUP_REPLICATION` clears the running membership (zero rows). Already running is errno **3093**. A catalog that cannot form three members is errno **3092**.

5. **`performance_schema.replication_group_members` stub.** Columns: `MEMBER_ID`, `MEMBER_HOST`, `MEMBER_PORT`, `MEMBER_STATE`, `MEMBER_ROLE`, `MEMBER_VERSION`. When started, `SELECT` returns three rows. When stopped, zero rows. Other GR P_S tables stay errno **1146**.

6. **Not full InnoDB Cluster** and not automatic failover, conflict certification, or distributed recovery. Clone remains M200.

## Consequences

- Agents clear `needs-human` for #352 once this ADR is on `main`.
- A test proves three-member membership after `START GROUP_REPLICATION`.
- Compatibility docs state this is a membership stub, not InnoDB Cluster / Oracle GR.
- M210 still requires later Phase Z work; accepting this ADR does not complete MySQL 8.0.

## Out of scope

- XCom / GCS from scratch
- Multi-primary mode and conflict certification
- Oracle Group Replication wire protocol
- InnoDB Cluster metadata / MySQL Shell admin API
- Clone plugin (M200)
- Claiming MySQL 8.0 compatibility is complete

## How to verify

```bash
cargo test -p rusql-storage group_replication
cargo test -p rusql-executor group_replication
cargo test -p rusql-server group_replication
```

```sql
START GROUP_REPLICATION;
SELECT MEMBER_ID, MEMBER_HOST, MEMBER_PORT, MEMBER_STATE, MEMBER_ROLE
  FROM performance_schema.replication_group_members;
STOP GROUP_REPLICATION;
```
