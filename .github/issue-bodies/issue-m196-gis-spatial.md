## Goal

Spatial types and a portable `ST_*` subset only after ADR (separate crate allowed only with ADR).

## Category

Phase Z — Remaining MySQL 8.0 surface (M196). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

needs-human. Not heap+WAL GIS. ADR must choose crate vs skip until later. Ultimate goal includes community GIS or an explicit skip — prefer implement subset over silent missing.


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] ADR: implement vs document skip for community GIS
- [ ] If implement: `POINT` storage + `ST_AsText`/`ST_GeomFromText` round-trip; mysql-diff fixture
- [ ] If skip: rusql-vs-mysql + SKIPS, and M209 matrix row is Out of scope only if community GIS is declared out — default is implement
- [ ] No new crate without ADR

## File Boundaries

Allowed:
- `docs/en/architecture/**`
- `crates/rusql-sql/src/**`
- `crates/rusql-core/src/types.rs`
- `crates/rusql-executor/src/expr.rs`
- `crates/*` new crate only with ADR
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Shipping a crate without ADR
- SRID 3857 full proj stack in v1

## Negative Constraints

- Do not implement geography vs geometry dual unless ADR
- Do not add spatial indexes in the same slice as types unless cheap

## Test plan

```bash
cargo test -p rusql-executor st_astext
# or docs-only verification if ADR says skip (must not be silent)
```
