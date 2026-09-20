## Goal

Likely skip: do not load arbitrary `.so` UDFs; document as out of scope unless an ADR says otherwise.

## Category

Phase Z — Remaining MySQL 8.0 surface (M201). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Security: loading .so is a trust boundary. Default skip with SKIPS + report. needs-human to confirm skip vs restricted loader.


**Sequencing**: not `agent-ready`. Phase X / ADR-gated work stays `needs-human` until an ADR is accepted. Do not start while M114 file boundaries overlap.

## Acceptance Criteria

- [ ] `CREATE FUNCTION … SONAME` errors with documented errno (or parser reject) — not silent ignore
- [ ] rusql-vs-mysql + SKIPS list UDF .so as out of scope (security)
- [ ] This skip does **not** complete the ultimate goal by itself

## File Boundaries

Allowed:
- `docs/en/reports/**`, `docs/zh-CN/reports/**`
- `tests/mysql-test/SKIPS.md`
- `crates/rusql-sql/src/**`
- `crates/rusql-i18n/**`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- dlopen of user .so in default builds
- New crates for a plugin VM without ADR

## Negative Constraints

- Do not skip SQL stored functions (M63)
- Do not claim plugin loader (M202) is done

## Test plan

```bash
cargo test -p rusql-sql udf_soname_rejected
```
