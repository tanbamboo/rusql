## Goal

Advertise `utf8mb4` / `utf8mb4_unicode_ci` in handshake and information_schema.

## Depends on

- M12 information_schema

## Acceptance Criteria

- [ ] Handshake charset 45 (utf8mb4)
- [ ] COLUMN_COLLATION in information_schema.columns

## File Boundaries

- crates/rusql-protocol/**, crates/rusql-executor/**
