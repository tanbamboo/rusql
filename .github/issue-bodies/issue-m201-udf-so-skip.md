## Goal

Explicit **security out-of-scope skip** (M188 pattern): do not load arbitrary `.so` UDFs. Document as out of scope.

## Category

Phase Z — Remaining MySQL 8.0 surface (M201). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Security: loading `.so` is a trust boundary. ADR accepted (`adr-m201-udf-so.md`): skip with SKIPS + compatibility report; `CREATE FUNCTION … SONAME` errors (errno **1064**). Not a restricted loader. Not M202.

## Acceptance Criteria

- [x] `CREATE FUNCTION … SONAME` errors with documented errno (or parser reject) — not silent ignore
- [x] rusql-vs-mysql + SKIPS list UDF .so as out of scope (security)
- [x] This skip does **not** complete the ultimate goal by itself

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
- Also if needed for executor reject: `crates/rusql-executor/src/**`, `crates/rusql-server/src/**`, `mysql-diff.json`

Forbidden:
- dlopen of user .so in default builds
- New crates for a plugin VM without ADR
- Claiming M202 plugin loader done

## Negative Constraints

- Do not skip SQL stored functions (M63)
- Do not claim plugin loader (M202) is done

## Test plan

```bash
cargo test -p rusql-sql udf_soname_rejected
```
