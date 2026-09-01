## Goal

`WHERE col < > <= >= !=` and `AND` of literals.

## Depends on

- M4 secondary indexes (point lookup path)

## Acceptance Criteria

- [ ] Comparison predicates filter rows
- [ ] `WHERE id = 1 AND name = 'x'` (AND)
- [ ] Compat tests

## File Boundaries

- crates/rusql-executor/**
