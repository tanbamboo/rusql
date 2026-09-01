## Goal

`SELECT * FROM a INNER JOIN b ON a.id = b.a_id` (two-table).

## Depends on

- M14 projection, M20 WHERE

## Acceptance Criteria

- [ ] INNER JOIN returns combined columns
- [ ] ON equality literal join
- [ ] Compat fixture

## File Boundaries

- crates/rusql-executor/**
