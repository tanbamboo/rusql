# M16: SELECT LIMIT

## Goal

Cap rows returned by `SELECT … LIMIT n` for pagination smoke tests.

## Acceptance criteria

- [x] Integer literal `LIMIT` on table `SELECT`
- [x] Works with column projection and `WHERE`
- [x] Tests + compat fixture

## Boundaries

- Literal limit only (no `OFFSET`, no parameterized limit)
