## Goal

Requirement-by-requirement MySQL 8.0 matrix in `rusql-vs-mysql.md` with **no Missing rows** for in-scope community features.

## Category

Phase Z — Remaining MySQL 8.0 surface (M209). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

**Definition of done for the ultimate goal** (with M210). Every roadmap row maps to Works / Stub / Missing / Out of scope. Missing is forbidden for in-scope items when this issue closes — therefore this issue closes last with M210.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] Matrix covers wire, SQL, metadata, security, replication listed in the full-parity roadmap
- [ ] No **Missing** for in-scope community MySQL 8.0; Out of scope only for M188/M201-class items
- [ ] zh-CN report in sync. HANDOFF must not say goal achieved until M210 also green

## File Boundaries

Allowed:
- `docs/en/reports/rusql-vs-mysql.md`
- `docs/zh-CN/reports/rusql-vs-mysql.md`
- `docs/en/specs/mysql-full-parity-roadmap.md`
- `docs/zh-CN/specs/mysql-full-parity-roadmap.md`
- `HANDOFF.md`
- `CHANGELOG.md`
- `docs/en/release-notes.md`
- `docs/zh-CN/release-notes.md`

Forbidden:
- Closing while any in-scope roadmap issue is open/missing
- CONSTITUTION.md
- Claiming drop-in without M210

## Negative Constraints

- Do not hide gaps as Stub without saying stub
- Do not count enterprise plugins as Missing if M188 listed Out of scope

## Test plan

```bash
node scripts/doc-parity.mjs
# human/agent checklist vs roadmap tables M36–M210
```
