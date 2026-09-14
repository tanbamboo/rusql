## Goal

Implement `INSERT … SELECT` and `INSERT … ON DUPLICATE KEY UPDATE` so clients can copy query results into a table and upsert on PRIMARY KEY conflict (MySQL semantics).

## Background

Phase Q after M67. HANDOFF and the full-parity roadmap list this as the next high-ROI client/ORM gap. Today `extract_insert_values` rejects non-`VALUES` sources (`INSERT requires VALUES`), and PRIMARY KEY duplicates are appended rather than rejected or updated.

## Acceptance Criteria

- [ ] `INSERT INTO dst (cols) SELECT … FROM src` inserts the selected rows
- [ ] `INSERT … SELECT` works with `WHERE`, expressions, `JOIN`, `UNION`, `ORDER BY` / `LIMIT`
- [ ] Column count mismatch returns a clear error
- [ ] Duplicate single-column `PRIMARY KEY` without `ON DUPLICATE KEY UPDATE` returns MySQL errno **1062**
- [ ] `ON DUPLICATE KEY UPDATE col = VALUES(col)` updates the existing PK row
- [ ] `ON DUPLICATE KEY UPDATE cnt = cnt + 1` reads the existing row on the right-hand side
- [ ] `INSERT … SELECT … ON DUPLICATE KEY UPDATE` upserts
- [ ] `INSERT IGNORE`, `REPLACE INTO`, PostgreSQL `ON CONFLICT`, and composite-PK ODKU return clear unsupported errors
- [ ] Unit/wire tests + `mysql-diff` suite
- [ ] Docs: CHANGELOG, release-notes, user-guide (en+zh-CN), roadmap Phase Q, HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**`
- `crates/rusql-i18n/**`
- `crates/rusql-server/src/**` (tests)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Storage engine redesign / new WAL record types
- UNIQUE secondary indexes
- `INSERT IGNORE` / `REPLACE INTO` implementation
- Statement-level implicit transactions
- New crates

## Negative Constraints

- Do not wrap `INSERT … SELECT` in an implicit transaction
- Do not implement `INSERT IGNORE` or `REPLACE`
- Do not implement UNIQUE secondary-index conflict detection
- Do not implement composite PRIMARY KEY `ON DUPLICATE KEY UPDATE` (detect + error 1062 still required)

## Test plan

```bash
cargo test -p rusql-sql insert_select
cargo test -p rusql-executor insert_select
cargo test -p rusql-executor on_duplicate
cargo test -p rusql-server insert_select
node scripts/mysql-diff.mjs
```
