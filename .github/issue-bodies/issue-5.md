## Goal

Draft an Architecture Decision Record (ADR) for MySQL replication support in rusql long-term roadmap.

## Acceptance Criteria

- [ ] ADR document in `docs/en/architecture/adr-replication.md`
- [ ] zh-CN mirror in `docs/zh-CN/architecture/adr-replication.md`
- [ ] Compares binlog-based vs consensus-based (Raft) approaches
- [ ] Recommends phased approach aligned with M6+ milestone

## File Boundaries

`docs/en/architecture/**`, `docs/zh-CN/architecture/**`

## Negative Constraints

- No replication code implementation in this issue
