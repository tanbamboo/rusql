## Goal

Implement MySQL-compatible privilege grants for database and table access.

## Category

Phase M — Security & privileges.

## Depends on

- M36 multi-schema catalog

## Acceptance Criteria

- [ ] `GRANT SELECT, INSERT ON db.* TO 'app'@'%'` persisted
- [ ] `REVOKE` removes privileges
- [ ] `SHOW GRANTS FOR 'app'@'%'` output shape
- [ ] Connection as `app` rejected for unauthorized statements (SQL error 1142)
- [ ] `mysql` user table stub in data directory or catalog

## File Boundaries

- `crates/rusql-core/**`, `crates/rusql-server/**`, `crates/rusql-executor/**`, `crates/rusql-i18n/**`

## Negative Constraints

- No row-level security
- No `WITH GRANT OPTION` in v1
