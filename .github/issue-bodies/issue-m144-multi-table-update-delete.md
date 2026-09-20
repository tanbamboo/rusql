## Goal

Accept two-table INNER JOIN forms of `UPDATE` and `DELETE` used by dumps and apps.

## Category

Phase S — JSON, set SQL, and remaining query forms (M144). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Single-table UPDATE/DELETE exist. MySQL `UPDATE t1 JOIN t2 ON … SET t1.c=…` and `DELETE t1 FROM t1 JOIN t2`. Limit this slice to two base tables and INNER JOIN.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `UPDATE t1 INNER JOIN t2 ON t1.id=t2.id SET t1.n = t2.n` updates matching t1 rows
- [ ] `DELETE t1 FROM t1 INNER JOIN t2 ON t1.id=t2.id` deletes matching t1 rows only
- [ ] Unknown alias/column errors. Single-table UPDATE/DELETE unchanged
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/**` (DML)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- FULL OUTER JOIN in DML
- Updating both tables in one statement
- Triggers beyond existing single-table hooks unless reuse is obvious

## Negative Constraints

- Do not implement `DELETE t1, t2 FROM` multi-target in this slice
- Do not add LIMIT on multi-table UPDATE

## Test plan

```bash
cargo test -p rusql-executor multi_update
cargo test -p rusql-executor multi_delete
```
