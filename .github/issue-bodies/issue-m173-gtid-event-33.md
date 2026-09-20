## Goal

Emit and apply `Gtid_log_event` (type 33) so replicas can follow GTID streams.

## Category

Phase W — Replication production (M173). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M58 GTID sets spike; M71–M74 row events. Type 33 is still later per Phase R. Replica applier must understand GTID-before-rows.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] Binlog stream includes event type 33 before row events when GTID is on (or documented always-on subset)
- [ ] Replica applies Gtid_log_event without aborting dump
- [ ] mysqlbinlog / dump test pins the header. Docs as usual

## File Boundaries

Allowed:
- `crates/rusql-storage/src/binlog.rs`
- `crates/rusql-storage/src/replica.rs`
- `crates/rusql-protocol/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Group Replication (M199)
- Heartbeat (M174) except leaving a hook
- Breaking existing TABLE_MAP/WRITE_ROWS

## Negative Constraints

- Do not implement anonymous GTID vs assigned beyond one documented mode
- Do not add gtid_purged in this slice (M176)

## Test plan

```bash
cargo test -p rusql-storage gtid_event
cargo test -p rusql-server gtid_dump
```
