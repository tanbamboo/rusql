## Goal

Persist VIRTUAL generated columns (`col AS (expr) [VIRTUAL]`) and compute them on SELECT; STORED stays later.

## Category

Phase T — Schema completeness (M146). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

mysqldump of simple schemas emits generated columns. Catalog + SELECT compute is the MVP. Do not persist generated values on INSERT for VIRTUAL.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `CREATE TABLE t (id INT PRIMARY KEY, n INT, g INT AS (n+1) VIRTUAL)` succeeds; `SELECT g` returns computed values
- [ ] INSERT/UPDATE of `g` is rejected (errno 3105 / documented). INSERT omit-g computes on read
- [ ] SHOW CREATE TABLE includes `AS (n + 1)` (or equivalent). STORED keyword may error with documented skip
- [ ] Additive catalog/WAL fields only (`serde(default)`)
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-core/src/**` (column meta)
- `crates/rusql-storage/src/**` (additive catalog/WAL)
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- STORED generated columns (persist on write)
- Functional indexes (M151)
- Breaking WAL field renames

## Negative Constraints

- Do not index VIRTUAL columns in this slice
- Do not claim MySQL generation expression function completeness — reuse existing expr

## Test plan

```bash
cargo test -p rusql-executor generated_column
cargo test -p rusql-storage generated_column
```
