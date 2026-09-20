## Goal

Send heartbeat events on a quiet `COM_BINLOG_DUMP` connection so dump stays alive.

## Category

Phase W — Replication production (M174). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

M73 live follow. Replicas disconnect without heartbeats. Master_heartbeat_period / replica_heartbeat documented subset.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] Idle dump connection receives heartbeat before replica timeout (test with a fake replica or unit clock)
- [ ] Row events still delivered. Heartbeat does not apply as DML
- [ ] Docs as usual

## File Boundaries

Allowed:
- `crates/rusql-storage/src/binlog.rs`
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
- GTID type 33 implementation (M173) except reuse
- Semi-sync (M178)

## Negative Constraints

- Do not change dump protocol version
- Do not send heartbeats on non-dump connections

## Test plan

```bash
cargo test -p rusql-server binlog_heartbeat
cargo test -p rusql-protocol heartbeat
```
