# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-15 |
| Branch | `main` (M145 merged — PR [#395](https://github.com/tanbamboo/rusql/pull/395) / #297) |
| Next step | Pick **M146 generated columns (VIRTUAL AS expr)** ([#298](https://github.com/tanbamboo/rusql/issues/298)) (`agent-ready`, first Phase T slice). Phase S **complete** on `main` (M133–M145). Ultimate MySQL 8.0 goal is **not** complete (Phases T–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-15)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M145 merge (mysql-diff suite `explain_json`) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** — last merge M113 PR #263 |
| Phase R (M114–M132) | **Complete** on `main` |
| Phase S (M133–M145) | **Complete** on `main` — last merge M145 PR #395 |
| Phases T–Z (M146–M210) | **In progress** — next sequenced: [M146 #298](https://github.com/tanbamboo/rusql/issues/298) (`agent-ready`). Milestones [T](https://github.com/tanbamboo/rusql/milestone/11)–[Z](https://github.com/tanbamboo/rusql/milestone/17); issues #298–#363 |

| Estimated surface | ~45–70% client-visible; remaining work is Phase T through Z (including M209/M210) |

## Recent Progress

- **M145 merged** — `EXPLAIN FORMAT=JSON SELECT …` returns one `EXPLAIN` column with documented JSON stub (`query_block` / `table` / `access_type`). SQL rewrite `FORMAT=` → `FORMAT` for sqlparser. Plain text EXPLAIN unchanged; ANALYZE still errors. mysql-diff suite `explain_json` (#297 / PR #395)
- **M144 merged** — Two-table `INNER JOIN` `UPDATE`/`DELETE` (#296 / PR #394)
- **M143 merged** — `INSERT INTO t SET col=expr, …` (#295 / PR #393)

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
