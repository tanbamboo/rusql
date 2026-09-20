## Goal

Explicit skip list for Oracle enterprise/closed-source plugins — not silent success.

## Category

Phase X — Security and TLS (M188). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Ultimate goal excludes closed-source enterprise plugins when listed. This issue is documentation + SKIPS only. needs-human to confirm the skip list.


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] `docs/en/reports/rusql-vs-mysql.md` (+ zh-CN) lists enterprise plugins as **out of scope** (audit, firewall, data masking, thread pool plugin, etc.)
- [ ] `tests/mysql-test/SKIPS.md` references the same list
- [ ] No code claims these plugins are loaded
- [ ] Roadmap M188 marked done when docs merged — still not MySQL 8.0 complete

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
