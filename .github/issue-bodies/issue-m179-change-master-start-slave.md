## Goal

Persist replica connection config and start/stop the applier (`CHANGE MASTER TO` / `START SLAVE` / `STOP SLAVE`).

## Category

Phase W — Replication production (M179). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M57 applier exists in spike form. Need SQL to set host/port/user and start dump. `CHANGE REPLICATION SOURCE TO` synonym for 8.0.23+.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `CHANGE MASTER TO MASTER_HOST=..., MASTER_PORT=..., MASTER_USER=...` persists; `START SLAVE` begins dump; `STOP SLAVE` stops
- [ ] `SHOW SLAVE STATUS` / `SHOW REPLICA STATUS` has IO/SQL running flags (subset columns documented)
- [ ] Bad host: documented error, not panic. Tests + docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-storage/src/replica.rs`
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- SSL replica options except ignore (M181 TLS is server listener)
- GTID auto-position unless M176 is on main

## Negative Constraints

- Do not implement MASTER_DELAY
- Do not store passwords in plaintext logs — catalog file perms documented

## Test plan

```bash
cargo test -p rusql-executor change_master
cargo test -p rusql-storage start_slave
```
