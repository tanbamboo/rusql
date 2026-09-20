## Goal

After Phase W: Group Replication or an explicit community-feature plan — not silent missing.

## Category

Phase Z — Remaining MySQL 8.0 surface (M199). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

needs-human. Ultimate goal includes replication HA. If GR is too large, ADR may sequence a documented primary-replica failover (M177) first and keep GR as a dated follow-on that still blocks M210 until done or explicitly out-of-scope (community GR is in-scope by default).


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] ADR: implement GR subset vs continue M177-only until GR lands
- [ ] A membership of 3 nodes or a documented blocker issue remaining open — M210 cannot close with GR unmarked
- [ ] Docs as usual

## File Boundaries

Allowed:
- `docs/en/architecture/**`
- `crates/rusql-storage/src/replica.rs`
- `crates/rusql-protocol/src/**`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Claiming InnoDB Cluster without membership tests
- Clone plugin (M200) in the same PR

## Negative Constraints

- Do not implement XCom from scratch without ADR
- Do not use silent skip

## Test plan

```bash
# integration: three rusql nodes or ADR-defined substitute harness
```
