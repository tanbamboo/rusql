## Goal

Expose connection/session list for admin clients (`SHOW PROCESSLIST`, `COM_PROCESS_INFO`).

## Category

Phase L — Wire protocol.

## Depends on

- M1 wire protocol

## Acceptance Criteria

- [ ] `SHOW PROCESSLIST` returns Id, User, Host, db, Command, Time, State, Info columns
- [ ] `COM_PROCESS_INFO` wire response compatible with official client
- [ ] `KILL QUERY connection_id` stub or full KILL (document scope)
- [ ] Integration test with `mysql -e "SHOW PROCESSLIST"`

## File Boundaries

- `crates/rusql-server/**`, `crates/rusql-executor/**`, `crates/rusql-core/**`

## Negative Constraints

- No performance_schema tables in this milestone
