## Goal

Do not silently ignore semi-sync replica ACKs: stub with documented no-wait or reject the plugin handshake.

## Category

Phase W — Replication production (M178). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Connectors/replicas may send semi-sync. Silent ignore is a false durability claim. Prefer explicit error or documented async-only ACK.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] Semi-sync dump request either: (a) returns a documented error, or (b) ACKs immediately with rusql-vs-mysql stating durability is still async WAL
- [ ] Normal dump without semi-sync unchanged
- [ ] Docs as usual

## File Boundaries

Allowed:
- `crates/rusql-protocol/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-storage/src/binlog.rs`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Claiming after_commit durability we do not have
- Group Replication

## Negative Constraints

- Do not implement rpl_semi_sync_master timeout waits in this slice unless tests exist
- Do not add plugins table rows pretending the plugin is loaded unless M202

## Test plan

```bash
cargo test -p rusql-protocol semisync
cargo test -p rusql-server semisync
```
