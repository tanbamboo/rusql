# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-30 |
| Branch | main |
| Next step | **M124 CREATE OR REPLACE VIEW** (#275) is next. Phase R issues #265–#283 filed. Phases S–Z filed (#285–#363, milestones 10–17) but not `agent-ready`. Ultimate MySQL 8.0 goal is **not** complete. |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-09-30)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (M123 PR; local mysql-diff 378 suite steps + 2 protocol smoke) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** — last merge M113 PR #263 |
| Phase R (M114–M132) | **M114–M123** on `main`; next M124 — GitHub milestone [Phase R](https://github.com/tanbamboo/rusql/milestone/9) |
| Phases S–Z (M133–M210) | **Filed** — milestones [S](https://github.com/tanbamboo/rusql/milestone/10)–[Z](https://github.com/tanbamboo/rusql/milestone/17); issues #285–#363. **Not** `agent-ready` |
| Estimated surface | ~45–70% client-visible; remaining work is Phase R+ through Z |

## Gaps (post-Q / Phase R)

Gap probe `scripts/mysql-gap-probe.mjs` on `main` after M113: **29 probes, 19 rusql gaps**, 9 ok, 1 both-fail (`CREATE PROCEDURE … IN` / `DELIMITER`). M114–M123 close ten of those gaps.

Session exit check (Docker `mysql:8.0` client → rusql): session introspection OK (Phase Q exit).

Remaining probe gaps now have issues: charset DDL (#265, **done** PR #346), JSON_EXTRACT (#266, **done** PR #365), UUID (#267, **done** PR #366), LAST_INSERT_ID(expr) (#268, **done** PR #367), GET_LOCK (#269, **done** PR #368), TABLE_CONSTRAINTS (#270, **done** PR #369), PROCESSLIST I_S (#271, **done** PR #370), PARAMETERS (#272, **done** PR #371), SHOW BINARY LOGS (#273, **done** PR #372), SHOW BINLOG EVENTS (#274, **done**), OR REPLACE VIEW (#275), text PREPARE (#276), SAVEPOINT (#277), WITH RECURSIVE (#278), INTERSECT (#279), window frames (#280), DISABLE ON SLAVE (#281), SHOW ENGINE INNODB STATUS (#282), procedure IN (#283). Later stages S–Z are filed (#285–#363): JSON pack, schema, locking, programs, replication, TLS, observability, remaining engine — not `agent-ready`. Ultimate goal still unmet.

## Recent Progress

- **M123 merged** — `SHOW BINLOG EVENTS` lists real events from known `{data_dir}/binlog/binlog.NNNNNN` files with columns `Log_name`, `Pos`, `Event_type`, `Server_id`, `End_log_pos`, `Info`. Optional `IN 'log_name'`, `FROM pos`, `LIMIT`. Missing directory is a documented empty list. Unknown `IN` files are errno 1220. Unknown event types are `Unknown` with empty Info. `SHOW BINARY LOGS` unchanged. Not mysqlbinlog tool compatibility. mysql-diff suite `show_binlog_events` uses `compare_output: false` (#274)
- **M122 merged** — `SHOW BINARY LOGS` / `SHOW MASTER LOGS` lists known `{data_dir}/binlog/binlog.NNNNNN` files with columns `Log_name`, `File_size` (on-disk sizes). Missing binlog directory is a documented empty list. Not mysqlbinlog tool compatibility. mysql-diff suite `show_binary_logs` uses `compare_output: false` (#273 / PR #372)
- **M121 merged** — `information_schema.PARAMETERS`: queryable catalog view (not errno 1146); portable columns `SPECIFIC_SCHEMA`, `SPECIFIC_NAME`, `ORDINAL_POSITION`, `PARAMETER_MODE`, `PARAMETER_NAME`, `DATA_TYPE`, `ROUTINE_TYPE`. Empty until M132 persists `IN` params; no invented rows; `SHOW CREATE PROCEDURE` still uses `()`. mysql-diff suite `information_schema_parameters` (#272 / PR #371)
- **M120 merged** — `information_schema.PROCESSLIST`: live session rows from the M53 registry; `SELECT ID` matches `CONNECTION_ID()`; I_S columns `ID`, `USER`, `HOST`, `DB`, `COMMAND`, `TIME`, `STATE`, `INFO`; SHOW PROCESSLIST columns unchanged (#271 / PR #370)
- **M119 merged** — `information_schema.TABLE_CONSTRAINTS`: PRIMARY KEY named `PRIMARY`; UNIQUE from `CREATE TABLE … UNIQUE` / `CREATE UNIQUE INDEX`; FOREIGN KEY from catalog names; portable columns `CONSTRAINT_SCHEMA`, `CONSTRAINT_NAME`, `TABLE_SCHEMA`, `TABLE_NAME`, `CONSTRAINT_TYPE`. CHECK not emitted (M147). Unknown I_S tables stay errno 1146. `KEY_COLUMN_USAGE` / `EVENTS` unchanged. mysql-diff suite `table_constraints` (#270 / PR #369)
- **M118 merged** — advisory `GET_LOCK` / `RELEASE_LOCK`: timeout 0 non-blocking; second connection gets `0` while held; `RELEASE_LOCK` is `1`/`0`/NULL like MySQL; NULL/empty name is errno 3057; names longer than 64 bytes are errno 1470; disconnect / `COM_RESET_CONNECTION` / `COM_CHANGE_USER` free that session’s names; `timeout>0` does not wait (M164). mysql-diff suite `get_lock` (#269 / PR #368)
- **M117 merged** — `LAST_INSERT_ID(expr)` setter: session value + later no-arg `LAST_INSERT_ID()`; nearest-integer (`5.9` → `6`, `5.5` → `6`) / 0 for non-numeric; connections isolated; reset/change-user clear; mysql-diff compares `LAST_INSERT_ID(5)` then `LAST_INSERT_ID()` (#268 / PR #367)
- **M116 merged** — `UUID()` RFC 4122 v4 hyphenated hex form; extra args i18n arity error; mysql-diff `compare_output: false` (#267 / PR #366)
- **M115 merged** — `JSON_EXTRACT(json, path)` for `$.key` / `$.a.b`; MySQL unquoted `1`; missing path NULL; invalid JSON errno 3141 (#266 / PR #365)
- **Phase S–Z filed** — 78 issues #285–#363 + milestones 10–17 (`node scripts/create-phase-s-z-issues.mjs`). Not `agent-ready` (sequencing). Phase X is `needs-human`.
- **M114 merged** — `CREATE DATABASE … CHARACTER SET … COLLATE …` persists per-schema charset; `SHOW CREATE DATABASE` / SCHEMATA use the catalog (#265 / PR #346)
- **Phase R filed** — issues #265–#283 + milestone 9; canonical plan expanded through M210
- **Phase Q complete** — filed table M62–M113 on `main`; session CLI exit verified (2026-09-20)
- **#263 merged** — M113 `SUBSTRING`/`ROUND`/`DATE_ADD` (#255)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
node scripts/mysql-gap-probe.mjs   # inventory only; not a CI gate
```
