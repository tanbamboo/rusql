# M14: SELECT column projection

## Goal

Return only requested columns from `SELECT` lists, not always full rows.

## Acceptance criteria

- [x] `SELECT id FROM users` single column
- [x] `SELECT name, id FROM users` preserves order
- [x] Works with `WHERE col = literal` and index path
- [x] `SELECT *` unchanged
- [x] Tests + docs

## Boundaries

- Column identifiers only (no expressions, aggregates, aliases beyond `AS`)
- No `tbl.col` qualified names except `CompoundIdentifier` last segment

## Decisions

| Topic | Choice |
|-------|--------|
| Projection | Post-scan row slice by catalog column index |
| Wildcard | `None` indices = pass-through all columns |
