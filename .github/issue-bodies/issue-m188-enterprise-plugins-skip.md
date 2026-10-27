## Goal

Explicit skip list for Oracle enterprise/closed-source plugins — not silent success.

## Category

Phase X — Security and TLS (M188). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

ADR accepted: [adr-m188-enterprise-plugins.md](../../docs/en/architecture/adr-m188-enterprise-plugins.md). Oracle Enterprise plugins listed in rusql-vs-mysql and SKIPS.md are **out of scope** and are not loaded. Community plugins required by Phases W–Y stay in scope. Not a plugin loader (M202). The ultimate MySQL 8.0 goal is not complete.

**Sequencing**: ADR accepted. Documentation + SKIPS only. Do not mass-label later phases.

## Acceptance Criteria

- [x] `docs/en/reports/rusql-vs-mysql.md` (+ zh-CN) lists enterprise plugins as **out of scope** (audit, firewall, data masking, thread pool plugin, etc.)
- [x] `tests/mysql-test/SKIPS.md` references the same list
- [x] No code claims these plugins are loaded
- [x] Roadmap M188 marked done when docs merged — still not MySQL 8.0 complete

## File Boundaries

Allowed:
- `docs/en/reports/**`, `docs/zh-CN/reports/**`
- `tests/mysql-test/SKIPS.md`
- `docs/en/specs/mysql-full-parity-roadmap.md`
- `docs/zh-CN/specs/mysql-full-parity-roadmap.md`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Implementing fake plugin loader success (M202)
- crates/ except i18n if a skip message is shown

## Negative Constraints

- Do not mark ultimate goal complete
- Do not skip community plugins that Phases W–Y still require

## Test plan

```bash
node scripts/doc-parity.mjs
# grep SKIPS.md for enterprise plugin names listed in the report
```
