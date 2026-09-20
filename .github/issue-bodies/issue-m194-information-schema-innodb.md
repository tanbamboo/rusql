## Goal

`information_schema.INNODB_TRX` (and a documented minimal set) is not errno 1146.

## Category

Phase Y — Observability (M194). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Monitoring queries INNODB_TRX / INNODB_LOCKS. Stub or live from M157 lock table. Name compatibility only.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `SELECT * FROM information_schema.INNODB_TRX` succeeds (0+ rows)
- [ ] List implemented INNODB_* tables in user-guide; others 1146 documented
- [ ] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-executor/src/info_schema.rs`
- `crates/rusql-storage/src/txn.rs` (read-only)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Creating real InnoDB tablespaces (M198)
- P_S (M189) except similar stub pattern

## Negative Constraints

- Do not invent lock IDs that collide with MySQL tools beyond documented subset
- Do not add SYS_* tables

## Test plan

```bash
cargo test -p rusql-executor innodb_trx
```
