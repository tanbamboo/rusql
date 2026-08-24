## Goal

Support additional MySQL 8.0 column types required by ORMs and Sysbench schema.

## Category

Phase H — DDL & catalog.

## Depends on

- M2 CREATE TABLE baseline

## Acceptance Criteria

- [ ] `DECIMAL(p,s)` parse, store, and wire-format round-trip
- [ ] `DATETIME` / `TIMESTAMP` (UTC-naive MVP acceptable with documented semantics)
- [ ] `TEXT` / `BLOB` variable-length types with reasonable size limits
- [ ] `JSON` type: store as string blob; `JSON_EXTRACT` deferred to M46
- [ ] `DESCRIBE` / `information_schema.COLUMNS` report correct `DATA_TYPE`
- [ ] `mysql-diff` cases for each new type

## File Boundaries

- `crates/rusql-core/**`, `crates/rusql-storage/**`, `crates/rusql-executor/**`, `crates/rusql-protocol/**`

## Negative Constraints

- No spatial/geometry types
- No fractional-second precision parity required for TIMESTAMP in v1
