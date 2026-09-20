# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-20 |
| Branch | issue-266-m115-json-extract |
| Next step | **M116 UUID** (#267). Label `agent-ready` after M115 merge. Phase R issues #265–#283 filed. Phases S–Z filed (#285–#363, milestones 10–17) but not `agent-ready`. Ultimate MySQL 8.0 goal is **not** complete. |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-09-20)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #346 M114) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** — last merge M113 PR #263 |
| Phase R (M114–M132) | **M114 on main (PR #346)**; **M115 in this PR** — GitHub milestone [Phase R](https://github.com/tanbamboo/rusql/milestone/9); next `agent-ready` [#267 M116](https://github.com/tanbamboo/rusql/issues/267) |
| Phases S–Z (M133–M210) | **Filed** — milestones [S](https://github.com/tanbamboo/rusql/milestone/10)–[Z](https://github.com/tanbamboo/rusql/milestone/17); issues #285–#363. **Not** `agent-ready` |
| Estimated surface | ~45–70% client-visible; remaining work is Phase R+ through Z |

## Gaps (post-Q / Phase R)

Gap probe `scripts/mysql-gap-probe.mjs` on `main` after M113: **29 probes, 19 rusql gaps**, 9 ok, 1 both-fail (`CREATE PROCEDURE … IN` / `DELIMITER`). M114 charset DDL and M115 `JSON_EXTRACT($.key)` close two of those gaps once merged.

Session exit check (Docker `mysql:8.0` client → rusql): session introspection OK (Phase Q exit).

Remaining probe gaps now have issues: charset DDL (#265, **done** PR #346), JSON_EXTRACT (#266, this PR), UUID (#267), LAST_INSERT_ID(expr) (#268), GET_LOCK (#269), TABLE_CONSTRAINTS (#270), PROCESSLIST I_S (#271), PARAMETERS (#272), SHOW BINARY LOGS/EVENTS (#273/#274), OR REPLACE VIEW (#275), text PREPARE (#276), SAVEPOINT (#277), WITH RECURSIVE (#278), INTERSECT (#279), window frames (#280), DISABLE ON SLAVE (#281), SHOW ENGINE INNODB STATUS (#282), procedure IN (#283). Later stages S–Z are filed (#285–#363): JSON pack, schema, locking, programs, replication, TLS, observability, remaining engine — not `agent-ready`. Ultimate goal still unmet.

## Recent Progress

- **M115** — `JSON_EXTRACT(json, path)` for `$.key` / `$.a.b`; MySQL unquoted `1`; missing path NULL; invalid JSON errno 3141 (#266)
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
