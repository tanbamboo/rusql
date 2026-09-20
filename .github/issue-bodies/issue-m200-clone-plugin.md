## Goal

`CLONE INSTANCE` / clone plugin equivalent for provisioning replicas, or documented community skip with a replacement procedure.

## Category

Phase Z — Remaining MySQL 8.0 surface (M200). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

needs-human. MySQL 8 clone plugin is community. Prefer file-snapshot + replica start documented procedure if plugin API is out; must not be silent.


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] Either: CLONE works on a fixture, or rusql-vs-mysql describes `mysqldump`/data-dir copy procedure as the supported substitute **and** M209 lists clone plugin as missing until implemented
- [ ] Default is implement a rusql clone/snapshot command — substitute only with needs-human ADR

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
