# HANDOFF — Cross-Session State



| Field | Value |

|-------|-------|

| Last updated | 2026-10-15 |

| Branch | `main` (M144 merged — PR [#394](https://github.com/tanbamboo/rusql/pull/394) / #296) |

| Next step | Pick **M145 EXPLAIN FORMAT=JSON stub** ([#297](https://github.com/tanbamboo/rusql/issues/297)) (`agent-ready`, last Phase S slice). Phase S nearly complete (M145 remains). Phases S–Z filed (#285–#363). Ultimate MySQL 8.0 goal is **not** complete (M209/M210 remain). |



## Ultimate goal



**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.



## Status vs goal (2026-10-15)



| Layer | Status |

|-------|--------|

| CI on `main` | Green after M144 merge (mysql-diff suite `multi_table_dml`) |

| Roadmap M36–M61 + PERF-B* | Complete |

| Phase Q (M62–M113) | **Complete** — last merge M113 PR #263 |

| Phase R (M114–M132) | **Complete** on `main` |

| Phases S–Z (M133–M210) | **In progress** — M133–M144 on `main`. Milestones [S](https://github.com/tanbamboo/rusql/milestone/10)–[Z](https://github.com/tanbamboo/rusql/milestone/17); issues #285–#363. Next sequenced: [M145 #297](https://github.com/tanbamboo/rusql/issues/297) (`agent-ready`) |

| Estimated surface | ~45–70% client-visible; remaining work is Phase S through Z (including M209/M210) |



## Recent Progress



- **M144 merged** — Two-table `INNER JOIN` `UPDATE`/`DELETE`: `UPDATE t1 INNER JOIN t2 ON … SET t1.c=t2.c` updates `t1` only; `DELETE t1 FROM t1 INNER JOIN t2 …` deletes from `t1` only. Single-table DML unchanged; unknown alias/column errors. Not multi-target delete, outer-join DML, or `UPDATE … FROM`. mysql-diff suite `multi_table_dml` (#296 / PR #394)

- **M143 merged** — `INSERT INTO t SET col=expr, …` rewritten to column-list `INSERT … VALUES` before parse; executor path unchanged from M68/M112. Unknown column same as column-list INSERT; duplicate PK errno 1062. `INSERT IGNORE … SET` skips PK conflicts. Not multi-table INSERT. mysql-diff suite `insert_set` (#295 / PR #393)

- **M142 merged** — `GREATEST`/`LEAST` vararg comparison (any NULL → NULL; arity &lt; 2 errno 1582; mixed quoted-string/numeric literals compare as strings). mysql-diff suite `greatest_least` (#294 / PR #392)



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


