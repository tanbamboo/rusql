## Goal

Close remaining `ALTER TABLE … ADD INDEX` / `DROP INDEX` / `ADD UNIQUE` forms used by mysqldump.

## Category

Phase T — Schema completeness (M153). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M38 ALTER extended exists but dumps still fail on some index DDL. Inventory current ALTER support; implement the missing dump forms only (ADD INDEX, DROP INDEX, ADD UNIQUE, DROP PRIMARY KEY if missing).


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `ALTER TABLE t ADD INDEX idx_n (n)` and `DROP INDEX idx_n ON t` / `ALTER TABLE t DROP INDEX idx_n` work
- [ ] `ALTER TABLE t ADD UNIQUE (n)` enforces uniqueness on later INSERT
- [ ] Existing ADD COLUMN / DROP COLUMN / RENAME unchanged
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/**` (alter)
- `crates/rusql-storage/src/**` (index catalog)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Generated columns (M146)
- ALGORITHM=INPLACE / LOCK=NONE except ignore-or-document
- Breaking WAL

## Negative Constraints

- Do not implement ALTER CHANGE COLUMN in this slice unless already started in M38 tests
- Do not add fulltext (M197)

## Test plan

```bash
cargo test -p rusql-executor alter_index
cargo test -p rusql-sql alter_index
```
