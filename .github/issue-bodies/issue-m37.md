## Goal

Implement MySQL-compatible `AUTO_INCREMENT` column option for integer primary keys.

## Category

Phase H — DDL & catalog.

## Depends on

- M36 multi-schema catalog (recommended), M23 PRIMARY KEY metadata

## Acceptance Criteria

- [ ] `CREATE TABLE t (id INT AUTO_INCREMENT PRIMARY KEY, …)` accepted
- [ ] `INSERT` omitting AUTO_INCREMENT column assigns monotonic next value
- [ ] `SHOW CREATE TABLE` emits `AUTO_INCREMENT=n` counter
- [ ] Counter survives server restart (WAL/catalog)
- [ ] `mysql-diff` step for AUTO_INCREMENT insert/select

## File Boundaries

- `crates/rusql-core/**`, `crates/rusql-executor/**`, `crates/rusql-storage/**`, `crates/rusql-i18n/**`

## Negative Constraints

- No `ALTER TABLE … AUTO_INCREMENT = n` reset in this milestone unless trivial
- No gap-free guarantees beyond MySQL-like best effort
