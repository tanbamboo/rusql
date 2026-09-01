## Goal

`DESCRIBE` / `SHOW COLUMNS` and minimal `information_schema.tables` / `information_schema.columns` for tooling compatibility.

## Acceptance Criteria

- [ ] `DESCRIBE tbl` returns Field/Type/Null/Key/Default/Extra
- [ ] `SHOW COLUMNS FROM tbl` same shape
- [ ] `SELECT * FROM information_schema.tables` lists catalog tables
- [ ] `SELECT * FROM information_schema.columns WHERE table_name = 'x'`
- [ ] Wire/executor tests + CHANGELOG + release notes

## File Boundaries

- crates/rusql-executor/**
- crates/rusql-server/compat/basic.json
- docs/**
