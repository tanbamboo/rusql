## Goal

Apply binlog events on a replica and serve `COM_BINLOG_DUMP` to downstream.

## Category

Phase N — Replication.

## Depends on

- M56 production binlog

## Acceptance Criteria

- [ ] Secondary rusql instance applies row events idempotently
- [ ] Primary accepts replica connection via `COM_BINLOG_DUMP` / `COM_REGISTER_SLAVE` minimum
- [ ] Integration test: primary INSERT visible on replica within timeout
- [ ] Fail replica gracefully on unsupported events

## File Boundaries

- `crates/rusql-server/**`, `crates/rusql-storage/**`, `crates/rusql-protocol/**`

## Negative Constraints

- No multi-source replication
- No automatic failover (M58)
