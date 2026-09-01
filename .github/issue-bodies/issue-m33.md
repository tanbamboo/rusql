## Goal

`CREATE VIEW v AS SELECT …` read-only queries.

## Depends on

- M22 JOIN

## Acceptance Criteria

- [ ] CREATE VIEW + SELECT FROM view
- [ ] information_schema.VIEWS stub

## File Boundaries

- crates/rusql-executor/**, crates/rusql-core/**
