## Goal

`CLONE INSTANCE` / clone plugin equivalent for provisioning replicas, or documented community skip with a replacement procedure.

## Category

Phase Z — Remaining MySQL 8.0 surface (M200). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

ADR accepted: file-level data-dir snapshot / `CLONE LOCAL DATA DIRECTORY` (local alias `CLONE INSTANCE DATA DIRECTORY`). Not a loaded Oracle `clone` plugin and not a `mysql.plugin` row. Remote `CLONE INSTANCE FROM` is out of this slice.

## Acceptance Criteria

- [x] CLONE works on a fixture (`cargo test -p rusql-storage clone_instance`)
- [x] Implement a rusql clone/snapshot command (ADR: [adr-m200-clone.md](../../docs/en/architecture/adr-m200-clone.md))

## File Boundaries

Allowed:
- `crates/rusql-server/src/**`
- `crates/rusql-storage/src/**`
- `docs/en/architecture/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Pretending plugin is loaded in mysql.plugins

## Negative Constraints

- Do not clone while holding global locks without docs
- Do not copy secrets extra files

## Test plan

```bash
cargo test -p rusql-storage clone_instance
```
