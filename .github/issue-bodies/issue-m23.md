## Goal

Catalog stores PRIMARY KEY and NOT NULL on columns (metadata + DESCRIBE).

## Depends on

- M2 CREATE TABLE

## Acceptance Criteria

- [ ] `PRIMARY KEY` parsed and shown in DESCRIBE Key column
- [ ] NOT NULL reflected in Null column

## File Boundaries

- crates/rusql-core/**, crates/rusql-executor/**
