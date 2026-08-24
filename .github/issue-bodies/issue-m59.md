## Goal

Implement correct string comparison and sort order for utf8mb4 collations (beyond metadata).

## Category

Phase O — Charset & collation.

## Depends on

- M35 utf8mb4 charset metadata

## Acceptance Criteria

- [ ] `ORDER BY name` on utf8mb4 column matches MySQL for `utf8mb4_unicode_ci`
- [ ] `WHERE name = '…'` uses collation-aware equality
- [ ] `SHOW COLLATION` / column collation in DESCRIBE consistent
- [ ] Portable corpus ≥10 strings including multi-byte characters
- [ ] Document supported collation list

## File Boundaries

- `crates/rusql-core/**`, `crates/rusql-executor/**`, `crates/rusql-i18n/**`

## Negative Constraints

- No charset conversion on the wire beyond utf8mb4 in v1
- No `utf8mb4_bin` binary collation unless trivial
