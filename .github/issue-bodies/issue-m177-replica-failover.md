## Goal

ADR + documented subset for promoting a replica (not Group Replication).

## Category

Phase W — Replication production (M177). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

needs-human. After M57/M176. Document steps: STOP SLAVE, RESET SLAVE, read-write promote. Implement only what the ADR lists.


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] ADR in architecture docs: failover subset vs MySQL
- [ ] A tested command sequence promotes a replica to accept writes (or documents manual ops)
- [ ] No silent split-brain. Docs + rusql-vs-mysql

## File Boundaries

Allowed:
- `docs/en/architecture/**`, `docs/zh-CN/architecture/**`
- `crates/rusql-storage/src/replica.rs`
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Group Replication (M199)
- Automatic orchestrator
- Force-push / data destroy in tests without temp dirs

## Negative Constraints

- Do not claim InnoDB Cluster
- Do not implement delayed replica

## Test plan

```bash
cargo test -p rusql-storage replica_promote
# plus documented manual steps in architecture ADR
```
