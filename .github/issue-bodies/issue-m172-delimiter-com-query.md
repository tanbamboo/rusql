## Goal

Allow official mysql CLI to create procedures (DELIMITER) or document a tested COM_QUERY-only path.

## Category

Phase V — Stored programs (M172). See [mysql-full-parity-roadmap.md](../../docs/en/specs/mysql-full-parity-roadmap.md).

## Background

Probe both-fail on DELIMITER. mysql CLI is a client-side delimiter; server COM_QUERY already receives full CREATE text. Either: accept no-op `DELIMITER //` as a stub statement, or document that CLI scripts need `--delimiter` / source without sending DELIMITER — pick the path that unblocks `mysql-test` sp*.


**Sequencing**: not `agent-ready` until prior-phase dependencies are on `main` and file boundaries do not overlap in-flight M114. Default: leave unlabeled.

## Acceptance Criteria

- [ ] Official `mysql` client can create a one-statement procedure used in tests (DELIMITER // … //) OR rusql-vs-mysql documents CLI limitation and wire COM_QUERY CREATE still works (M132)
- [ ] Unknown client DELIMITER must not abort the session
- [ ] Tests: at least one mysql CLI or mysql-test sp case. Docs as usual

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-server/src/mysql_test_subset.rs`
- `tests/mysql-test/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`
- `scripts/create-phase-s-z-issues.mjs`
- `scripts/phase-s-z-catalog.mjs`

Forbidden:
- Changing COM_QUERY framing
- Multi-result compound scripts beyond procedure create
- Auth/TLS

## Negative Constraints

- Do not implement mysql CLI inside the server
- Do not treat `;` inside strings as delimiters incorrectly

## Test plan

```bash
cargo test -p rusql-server delimiter_stub
node scripts/mysql-test-subset.mjs
```
