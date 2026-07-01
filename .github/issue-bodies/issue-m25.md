## Goal

Binary protocol column metadata + rows for `COM_STMT_EXECUTE`.

## Depends on

- M11 prepared statements

## Acceptance Criteria

- [ ] Drivers requesting binary resultset get correct types
- [ ] Protocol unit tests

## File Boundaries

- crates/rusql-protocol/**, crates/rusql-server/**
