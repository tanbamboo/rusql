## Goal

Return live file + position from WAL/binlog (not a stub) for `SHOW MASTER STATUS` / `SHOW BINARY LOG STATUS`.

## Category

Phase W — Replication production (M175). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M122 SHOW BINARY LOGS lists files. Ops need File/Position. MySQL 8.4 renamed to SHOW BINARY LOG STATUS — accept both names.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] After INSERT, SHOW MASTER STATUS Position advances vs before INSERT
- [ ] Columns include File, Position (Binlog_Do_DB etc. may be empty stubs)
- [ ] SHOW BINARY LOGS still works (M122). Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/**`
- `crates/rusql-storage/src/binlog.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- CHANGE MASTER (M179)
- Inventing binlog files that do not exist

## Negative Constraints

- Do not claim GTID executed in this result (M176 uses @@gtid_executed)
- Do not add SHOW SLAVE STATUS here

## Test plan

```bash
cargo test -p rusql-executor show_master_status
cargo test -p rusql-server show_master_status
```
