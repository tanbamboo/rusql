## Goal

Declare and enforce `FOREIGN KEY` referential constraints on DML.

## Category

Phase H — DDL & catalog.

## Depends on

- M38 extended ALTER, M36 multi-schema (optional)

## Acceptance Criteria

- [ ] `CREATE TABLE … FOREIGN KEY (col) REFERENCES parent(id)` in CREATE/ALTER
- [ ] `INSERT`/`UPDATE` violating FK rejected with MySQL-compatible SQL error
- [ ] `DELETE` on parent rejected or cascades per `ON DELETE` clause (RESTRICT minimum)
- [ ] `information_schema.KEY_COLUMN_USAGE` stub rows for FKs
- [ ] Portable negative tests in compat harness

## File Boundaries

- `crates/rusql-core/**`, `crates/rusql-executor/**`, `crates/rusql-storage/**`, `crates/rusql-i18n/**`

## Negative Constraints

- No `ON UPDATE CASCADE` required unless RESTRICT path done first
- No cross-engine FK metadata beyond rusql catalog
