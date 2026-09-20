## Goal

Evaluate `SUBSTRING` / `SUBSTR`, `ROUND`, and `DATE_ADD` so ORM/SQL that uses these builtins no longer returns `unsupported function` / `unsupported SELECT expression`.

## Background

Phase Q expressions after M46 (`CONCAT`, `COALESCE`, `CAST`, `NOW`/`CURDATE`, `LENGTH`/`LOWER`/`UPPER`). Probe (`scripts/mysql-gap-probe.mjs`, 2026-09-20): `fn_substring` → unsupported `Substring` AST; `fn_round` → unsupported `ROUND`; `fn_date_add` → unsupported `DATE_ADD`. Leave `JSON_EXTRACT`, `UUID()`, `GET_LOCK`, and `LAST_INSERT_ID(expr)` for later slices.

## Acceptance Criteria

- [x] `SELECT SUBSTRING('abc', 1, 2)` and `SELECT SUBSTR('abc', 1, 2)` return `ab` (MySQL 1-based)
- [x] `SELECT ROUND(1.4)` returns a numeric cell comparable to MySQL (`1`); `ROUND(1.5)` follows MySQL rounding for this slice (document if half-away-from-zero vs banker's)
- [x] `SELECT DATE_ADD('2026-01-01', INTERVAL 1 DAY)` returns `2026-01-02` (DATE or DATETIME string). Support `DAY` / `HOUR` / `MINUTE` / `SECOND` units already used by event intervals; `MONTH`/`YEAR` may use the same 30/365-day approximation as M105 or a documented calendar add
- [x] Unknown other functions still return unsupported. M46 builtins unchanged
- [x] Unit/wire tests; `mysql-diff` (compare SUBSTRING/ROUND; DATE_ADD can compare); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**` (`expr.rs`)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- `JSON_EXTRACT` / `UUID` / `GET_LOCK` / `LAST_INSERT_ID(expr)`
- Window frames / `WITH RECURSIVE` / `INTERSECT`
- New crates

## Negative Constraints

- Do not claim full MySQL date-arithmetic calendar (leap months) unless tests pin a documented rule
- Do not implement `SUBSTRING_INDEX` / `MID` unless a one-line alias of SUBSTRING
- Do not implement GTID event type 33 / heartbeat

## Test plan

```bash
cargo test -p rusql-executor substring
cargo test -p rusql-executor round
cargo test -p rusql-executor date_add
cargo test -p rusql-server substring
```
