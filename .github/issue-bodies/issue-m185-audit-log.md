## Goal

Optional JSON audit file for connect and query events (not enterprise plugin).

## Category

Phase X — Security and TLS (M185). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

needs-human (ops/privacy). Flag `--audit-log=` path. JSON lines: timestamp, user, command, status. Off by default.


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] With flag, a successful COM_QUERY appends one JSON line; connect/disconnect logged
- [ ] Without flag, no file created. Rotation not required
- [ ] Docs + warning that this is not Oracle audit plugin

## File Boundaries

Allowed:
- `crates/rusql-server/src/**`
- `crates/rusql-protocol/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Claiming enterprise audit compatibility
- Logging password packets

## Negative Constraints

- Do not audit resultset contents
- Do not add performance_schema in this slice (M189)

## Test plan

```bash
cargo test -p rusql-server audit_log
```
