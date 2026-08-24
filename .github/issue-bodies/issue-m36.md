## Goal

Support `CREATE DATABASE` / `DROP DATABASE` and multiple logical schemas in the catalog (not only the fixed `rusql` schema).

## Category

Phase H — DDL & catalog. See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Depends on

- M31 durable WAL, M15 USE database (single-schema today)

## Acceptance Criteria

- [ ] `CREATE DATABASE app_db` persists schema metadata across restart
- [ ] `USE app_db` switches session default schema; `SHOW DATABASES` lists all
- [ ] `DROP DATABASE app_db` removes schema and tables; errors if not empty (MySQL-compatible mode)
- [ ] `mysql-diff` suite extended with multi-schema portable steps
- [ ] i18n error messages for unknown/duplicate database

## File Boundaries

- `crates/rusql-core/**`, `crates/rusql-executor/**`, `crates/rusql-storage/**`, `crates/rusql-server/**`, `crates/rusql-i18n/**`

## Negative Constraints

- Do not implement cross-database JOIN in this milestone
- Do not change default `--data-dir` layout without ADR note

## Manual test

```bash
mysql -h 127.0.0.1 -P 3307 -u root --protocol=TCP -e "CREATE DATABASE testdb; USE testdb; CREATE TABLE t (id INT PRIMARY KEY); SHOW TABLES;"
```

Expected: table `t` visible in `testdb`.
