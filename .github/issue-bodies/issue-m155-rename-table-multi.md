## Goal

Accept `RENAME TABLE a TO b, c TO a` atomic multi-pair swaps.

## Category

Phase T — Schema completeness (M155). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Single rename may already exist. MySQL swap idiom uses two pairs in one statement (atomic). Failure must not leave one rename applied.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `RENAME TABLE t1 TO t1_old, t2 TO t1` results in original t2 rows under name t1 and t1 under t1_old
- [ ] Unknown name errno 1146; conflict errno 1050. Partial apply on error is forbidden
- [ ] Unit/wire tests; `mysql-diff`; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/**`
- `crates/rusql-storage/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Cross-database rename unless already supported and tested
- View rename unless reuse is obvious
- Breaking WAL

## Negative Constraints

- Do not rename information_schema
- Do not implement ALTER TABLE RENAME as a second syntax unless alias

## Test plan

```bash
cargo test -p rusql-storage rename_table
cargo test -p rusql-executor rename_table
```
