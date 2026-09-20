## Goal

Best-effort rusql txn/lock snapshot in `SHOW ENGINE INNODB STATUS` after M131 stub.

## Category

Phase Y — Observability (M193). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M131 stub text. This slice fills TRANSACTIONS / LOCKS from rusql txn manager (not Oracle mutex names). Document that engine name is compatibility.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] During an open transaction, STATUS text includes that connection/trx (or documented rusql section)
- [ ] Stub header still MySQL-shaped enough for clients that only check acceptance
- [ ] M131 column names unchanged. Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-storage/src/txn.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Fake InnoDB mutex counters
- information_schema.INNODB_* (M194)

## Negative Constraints

- Do not claim buffer pool stats
- Do not parse-compatible every InnoDB section

## Test plan

```bash
cargo test -p rusql-executor innodb_status_live
```
