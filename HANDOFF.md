# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-10-15 |
| Branch | `issue-297-m145-explain-format-json` (M145 in progress — PR pending) |
| Next step | Merge M145 PR; label **M146 generated columns** ([#298](https://github.com/tanbamboo/rusql/issues/298)) `agent-ready` (first Phase T slice). Ultimate MySQL 8.0 goal is **not** complete (Phases T–Z including M209/M210 remain). |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.

## Status vs goal (2026-10-15)

| Layer | Status |
|-------|--------|
| CI on `main` | Green after M144 merge (mysql-diff suite `multi_table_dml`) |
| Roadmap M36–M61 + PERF-B* | Complete |
| Phase Q (M62–M113) | **Complete** — last merge M113 PR #263 |
| Phase R (M114–M132) | **Complete** on `main` |
| Phases S–Z (M133–M210) | **In progress** — M133–M144 on `main`; M145 on branch #297. Milestones [S](https://github.com/tanbamboo/rusql/milestone/10)–[Z](https://github.com/tanbamboo/rusql/milestone/17); issues #285–#363. After M145 merge: label [M146 #298](https://github.com/tanbamboo/rusql/issues/298) `agent-ready` |

| Estimated surface | ~45–70% client-visible; remaining work is Phase T through Z (including M209/M210) |

## Recent Progress

- **M145 (branch)** — `EXPLAIN FORMAT=JSON SELECT …` returns one `EXPLAIN` column with documented JSON stub (`query_block` / `table` / `access_type`). SQL rewrite `FORMAT=` → `FORMAT` for sqlparser. Plain text EXPLAIN unchanged; ANALYZE still errors. mysql-diff suite `explain_json` (#297)
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
