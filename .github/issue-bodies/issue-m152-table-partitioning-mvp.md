## Goal

MVP `PARTITION BY RANGE` with equality/range prune on the partition key after an ADR for on-disk layout.

## Category

Phase T — Schema completeness (M152). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Roadmap: RANGE HASH KEY LIST — this issue is RANGE only. ADR required before a new storage module. HASH/KEY/LIST stay later. Human review of ADR before implement (needs-human until ADR accepted).


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] ADR in `docs/en/architecture/` (zh-CN mirror) describing partition catalog + prune
- [ ] `CREATE TABLE t (id INT PRIMARY KEY) PARTITION BY RANGE (id) (PARTITION p0 VALUES LESS THAN (10), PARTITION p1 VALUES LESS THAN MAXVALUE)` succeeds after ADR
- [ ] `SELECT` with `id = 5` does not scan p1 (test via EXPLAIN or partition metrics)
- [ ] Without ADR, do not merge storage layout changes
- [ ] Docs as usual; tests after ADR

## File Boundaries

Allowed:
- `docs/en/architecture/**`, `docs/zh-CN/architecture/**`
- `crates/rusql-sql/src/**`
- `crates/rusql-storage/src/**` (new module only with ADR)
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- HASH/KEY/LIST partitioning
- New crates without ADR
- Silent full-table scan claiming prune

## Negative Constraints

- Do not subpartition
- Do not implement PARTITION exchange

## Test plan

```bash
cargo test -p rusql-storage partition_range
cargo test -p rusql-executor partition_range
```
