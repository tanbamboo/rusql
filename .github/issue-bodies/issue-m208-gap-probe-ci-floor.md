## Goal

Promote `mysql-gap-probe.mjs` to a CI floor: 0 unexpected rusql-only gaps.

## Category

Phase Z — Remaining MySQL 8.0 surface (M208). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Probe is inventory-only today. After Phase R probe set is closed, remaining gaps must be listed in an allowfile. Unexpected new gaps fail CI.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] CI job runs gap probe vs Docker mysql:8.0 when Docker available; otherwise skip with explicit job skip
- [ ] Allowfile of known gaps; empty unexpected list required
- [ ] Docs: how to add a gap when filing a new issue

## File Boundaries

Allowed:
- `scripts/mysql-gap-probe.mjs`
- `scripts/**` allowfile
- `.github/workflows/**`
- `tests/**`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Making probe pass by deleting probes
- Requiring Docker on every contributor without skip

## Negative Constraints

- Do not treat both-fail as rusql-only
- Do not mark M210 done from this issue alone

## Test plan

```bash
node scripts/mysql-gap-probe.mjs
node scripts/harness-validate.mjs
```
