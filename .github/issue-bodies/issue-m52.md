## Goal

Implement remaining prepared-statement and metadata protocol commands used by drivers.

## Category

Phase L — Wire protocol.

## Depends on

- M11 prepared statements, M25 binary resultset

## Acceptance Criteria

- [ ] `COM_FIELD_LIST` returns column metadata for table (legacy clients)
- [ ] `COM_STMT_RESET` clears bound parameters for stmt_id
- [ ] `COM_STMT_SEND_LONG_DATA` chunks for large param blobs (minimum viable)
- [ ] JDBC/mysql2 prepared statement long-data path smoke test documented
- [ ] No hang on unsupported command — proper ERR for unsupported subsets

## File Boundaries

- `crates/rusql-protocol/**`, `crates/rusql-server/**`

## Negative Constraints

- No `COM_STMT_FETCH` cursor fetch unless required by smoke test
