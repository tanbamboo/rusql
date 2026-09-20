## Goal

Persist `@@gtid_executed` (and handshake/session read) for the replica/primary GTID set.

## Category

Phase W — Replication production (M176). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M58/M79 session vars. After M173, executed set must survive restart. Format: uuid:intervals text like MySQL.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `SELECT @@global.gtid_executed` returns the set after applying/producing GTIDs
- [ ] Restart keeps the set. Handshake optional SESSION_TRACK or variable catalog includes it
- [ ] Empty server returns empty string not unsupported
- [ ] Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-storage/src/**`
- `crates/rusql-executor/src/session_var.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-protocol/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- gtid_mode OFF/ON switching UI beyond documented default
- Group Replication sets

## Negative Constraints

- Do not implement GTID subtract functions except as needed for the string format
- Do not add mysql.gtid_executed table unless cheap

## Test plan

```bash
cargo test -p rusql-storage gtid_executed
cargo test -p rusql-executor gtid_executed
```
