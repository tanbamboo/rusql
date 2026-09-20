## Goal

Session-scoped `LOCK TABLES t READ|WRITE` and `UNLOCK TABLES`.

## Category

Phase U — Transactions, locking, isolation (M163). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

mysqldump and some apps use LOCK TABLES. Session holds named table locks; other sessions wait on WRITE. Implicit commit on LOCK TABLES per MySQL.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] `LOCK TABLES t WRITE` then another session’s INSERT into t waits/times out until `UNLOCK TABLES`
- [ ] `LOCK TABLES t READ` allows concurrent READ, blocks writers
- [ ] Unknown table errno 1146. Implicit commit of existing txn documented with a test
- [ ] Unit/wire tests; docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-executor/src/**`
- `crates/rusql-storage/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- FLUSH TABLES WITH READ LOCK (global) unless a tiny extra with tests
- Locking views/temporary tables beyond documented skip
- WAL format breaks

## Negative Constraints

- Do not implement LOCK TABLES … LOCAL INNODB
- Do not add GET_LOCK wait here (M164)

## Test plan

```bash
cargo test -p rusql-executor lock_tables
cargo test -p rusql-server lock_tables
```
