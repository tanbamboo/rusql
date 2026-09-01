## Goal

Extend `ALTER TABLE` beyond `ADD COLUMN` to cover common migration patterns.

## Category

Phase H — DDL & catalog.

## Depends on

- M24 ALTER ADD COLUMN, M23 PRIMARY KEY metadata

## Acceptance Criteria

- [ ] `ALTER TABLE t DROP COLUMN c`
- [ ] `ALTER TABLE t MODIFY COLUMN c …` (type/nullability rename within column)
- [ ] `ALTER TABLE t RENAME COLUMN old TO new` or MySQL `CHANGE COLUMN` equivalent
- [ ] `ALTER TABLE t RENAME TO new_name`
- [ ] Catalog + WAL reflect changes; `DESCRIBE` / `SHOW CREATE TABLE` updated
- [ ] At least 3 portable cases in `mysql-diff`

## File Boundaries

- `crates/rusql-executor/**`, `crates/rusql-core/**`, `crates/rusql-storage/**`

## Negative Constraints

- No online DDL / inplace algorithm flags required
- No partition ALTER in this milestone
