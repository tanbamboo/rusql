## Goal

Add MySQL 8.0 default collation `utf8mb4_0900_ai_ci` for compare/sort (completes M59 exit criteria).

## Category

Phase O — Charset & collation (post-M61).

## Depends on

- M59 `utf8mb4_unicode_ci`

## Acceptance Criteria

- [ ] `Collation::Utf8Mb4_0900AiCi` with `compare` / `eq` wired into executor
- [ ] `SHOW COLLATION` lists `utf8mb4_0900_ai_ci`
- [ ] Portable corpus ≥10 strings; sort order matches MySQL 8.0 for sample set
- [ ] `ORDER BY` and `WHERE =` respect column collation when set to `utf8mb4_0900_ai_ci`

## File Boundaries

- `crates/rusql-core/**`, `crates/rusql-executor/**`, `crates/rusql-i18n/**`

## Negative Constraints

- No full ICU integration; use documented approximation acceptable for MVP if corpus passes
- No charset conversion beyond utf8mb4
