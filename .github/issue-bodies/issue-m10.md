## Goal

`SHOW TABLES` and `SHOW DATABASES` for client/schema discovery.

## Acceptance Criteria

- [ ] `SHOW TABLES` returns `Tables_in_rusql` column
- [ ] `SHOW DATABASES` returns `rusql`
- [ ] Compat `show_tables` suite passes

## File Boundaries

- crates/rusql-executor/**
- crates/rusql-storage/**
- crates/rusql-server/compat/basic.json
