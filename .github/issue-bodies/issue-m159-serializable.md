## Goal

Treat `SERIALIZABLE` as `FOR UPDATE` on reads (documented) or reject with a documented errno.

## Category

Phase U — Transactions, locking, isolation (M159). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

After M157/M158. MySQL SERIALIZABLE uses next-key locks; rusql may map reads to FOR UPDATE waits as an MVP if documented, or return errno until M161.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `SET TRANSACTION ISOLATION LEVEL SERIALIZABLE` is accepted (not ignored)
- [ ] Either: a concurrent committed insert is not visible as a phantom on a scanned range (documented FOR UPDATE-on-read), OR: statements error with documented errno and rusql-vs-mysql lists the skip
- [ ] RR/RC from M158 unchanged
- [ ] Docs + tests for the chosen path

## File Boundaries

Allowed:
- `crates/rusql-storage/src/**`
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Full InnoDB next-key without ADR (M161)
- XA

## Negative Constraints

- Do not silently keep snapshot-only SERIALIZABLE
- Do not change default isolation

## Test plan

```bash
cargo test -p rusql-storage serializable
cargo test -p rusql-server serializable
```
