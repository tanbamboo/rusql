## Goal

UPDATE/DELETE row events match mysqlbinlog v2 / partial row images for the DML subset.

## Category

Phase W — Replication production (M180). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M74 UPDATE/DELETE rows. MySQL uses v2 with optional partial images (binlog_row_image). Pin MINIMAL or FULL and document.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] mysqlbinlog (or rusql decoder test) round-trips UPDATE of one column without corrupting unchanged columns on apply
- [ ] WRITE_ROWS insert path unchanged. Document binlog_row_image value
- [ ] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-storage/src/binlog.rs`
- `crates/rusql-storage/src/replica.rs`
- `crates/rusql-protocol/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Statement-based replication
- Compressed row events

## Negative Constraints

- Do not implement binlog_row_value_options
- Do not change TABLE_MAP column types except to match v2 headers

## Test plan

```bash
cargo test -p rusql-storage row_event_v2
cargo test -p rusql-storage update_rows_partial
```
