## Goal

Accept table/column `CHECK (expr)` and reject violating INSERT/UPDATE with errno 3819.

## Category

Phase T — Schema completeness (M147). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M39 FOREIGN KEY exists. CHECK is dump- and ORM-common. Evaluate expr as boolean per row. Constraint names optional.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `CREATE TABLE t (id INT PRIMARY KEY, n INT CHECK (n > 0))` then `INSERT … n=0` fails errno 3819 (i18n)
- [ ] Valid rows insert. Named `CONSTRAINT c CHECK` stored; `information_schema.TABLE_CONSTRAINTS` may wait for M119 if not on main — then SHOW CREATE TABLE must still show CHECK
- [ ] Additive catalog only
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-core/src/**`
- `crates/rusql-storage/src/**` (additive)
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- `CHECK` ENFORCED/NOT ENFORCED except reject NOT ENFORCED or document no-op
- Domain types
- Breaking WAL

## Negative Constraints

- Do not evaluate CHECK on SELECT
- Do not implement ALTER ADD CHECK unless it reuses CREATE path and tests pin it

## Test plan

```bash
cargo test -p rusql-executor check_constraint
cargo test -p rusql-sql check_constraint
```
