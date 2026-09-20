## Goal

ADR: crash recovery equivalent to InnoDB tablespaces — not heap+WAL alone as a false claim.

## Category

Phase Z — Remaining MySQL 8.0 surface (M198). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

needs-human. rusql WAL is not InnoDB. Ultimate goal needs documented recovery: either evolve storage or honestly describe equivalent guarantees and remaining gaps in M209.


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] ADR comparing rusql WAL+heap vs InnoDB tablespace recovery (redo, doublewrite, .ibd)
- [ ] Implementation tasks split or a recovery test: kill -9 after COMMIT, data present
- [ ] Cannot mark M210 done without this ADR accepted

## File Boundaries

Allowed:
- `docs/en/architecture/**`, `docs/zh-CN/architecture/**`
- `crates/rusql-storage/src/**`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Renaming rusql to InnoDB in SHOW ENGINES without behavior
- CONSTITUTION.md

## Negative Constraints

- Do not claim .ibd file compatibility
- Do not drop WAL

## Test plan

```bash
cargo test -p rusql-storage crash_recovery
# plus ADR review checklist
```
