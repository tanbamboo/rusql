## Goal

MySQL 8 component/plugin loader subset or documented reject of `INSTALL PLUGIN`.

## Category

Phase Z — Remaining MySQL 8.0 surface (M202). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

needs-human. Community plugins (clone, group_replication) need a loader story. Minimal: INSTALL PLUGIN errors with i18n until real loader ADR.


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] `INSTALL PLUGIN` / `UNINSTALL PLUGIN` either work for a rusql-native plugin or error documented (not OK packet success)
- [ ] `mysql.plugin` table exists or 1146 documented
- [ ] ADR if implementing a loader

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `docs/en/architecture/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Loading Oracle closed-source plugins
- UDF .so (M201) via this loader without ADR

## Negative Constraints

- Do not report INSTALL PLUGIN OK without loading anything
- Do not add audit plugin fake

## Test plan

```bash
cargo test -p rusql-executor install_plugin
```
