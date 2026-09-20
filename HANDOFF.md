# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-20 |
| Branch | main |
| Next step | **M114 in flight** (`CREATE DATABASE … CHARACTER SET`, #265, `agent-ready` P0). After merge: label M115 `agent-ready`. Phase R issues #265–#283 filed. Ultimate MySQL 8.0 goal is **not** complete. |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-09-20)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #263 / run [35499832912](https://github.com/tanbamboo/rusql/actions/runs/35499832912)) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** — last merge M113 PR #263 |
| Phase R (M114–M132) | **Filed** — GitHub milestone [Phase R](https://github.com/tanbamboo/rusql/milestone/9); first `agent-ready` [#265 M114](https://github.com/tanbamboo/rusql/issues/265) |
| Phases S–Z | Specified in roadmap (not filed as GitHub issues yet) |
| Estimated surface | ~45–70% client-visible; remaining work is Phase R+ |

## Gaps (post-Q / Phase R)

Gap probe `scripts/mysql-gap-probe.mjs` on `main` after M113: **29 probes, 19 rusql gaps**, 9 ok, 1 both-fail (`CREATE PROCEDURE … IN` / `DELIMITER`).

Session exit check (Docker `mysql:8.0` client → rusql): session introspection OK (Phase Q exit).

Remaining probe gaps now have issues: charset DDL (#265), JSON_EXTRACT (#266), UUID (#267), LAST_INSERT_ID(expr) (#268), GET_LOCK (#269), TABLE_CONSTRAINTS (#270), PROCESSLIST I_S (#271), PARAMETERS (#272), SHOW BINARY LOGS/EVENTS (#273/#274), OR REPLACE VIEW (#275), text PREPARE (#276), SAVEPOINT (#277), WITH RECURSIVE (#278), INTERSECT (#279), window frames (#280), DISABLE ON SLAVE (#281), SHOW ENGINE INNODB STATUS (#282), procedure IN (#283). Later: locking, GIS, GTID 33, TLS, performance_schema — Phases S–Z.

## Recent Progress

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
