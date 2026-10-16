# HANDOFF — Cross-Session State



| Field | Value |

|-------|-------|

| Last updated | 2026-10-16 |

| Branch | `issue-308-m156-columns-extra` (M156 PR pending, #308) |

| Next step | Merge M156 PR; label **M157 #309** `agent-ready` (first Phase U). Ultimate MySQL 8.0 goal is **not** complete (Phases U–Z including M209/M210 remain). |



## Ultimate goal



**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md). Not achieved until Phase Z M209/M210 evidence.



## Status vs goal (2026-10-16)



| Layer | Status |

|-------|--------|

| CI on `main` | Pending M156 merge (mysql-diff `information_schema_columns_extra`) |

| Roadmap M36–M61 + PERF-B* | Complete |

| Phase Q (M62–M113) | **Complete** |

| Phase R (M114–M132) | **Complete** on `main` |

| Phase S (M133–M145) | **Complete** on `main` |

| Phase T (M146–M156) | M156 in PR #308; prior slices on `main` — Phase T exit (simple mysqldump load) still open after M156 |

| Phases U–Z (M157–M210) | **Next** — [M157 #309](https://github.com/tanbamboo/rusql/issues/309); milestones [U](https://github.com/tanbamboo/rusql/milestone/12)–[Z](https://github.com/tanbamboo/rusql/milestone/17); issues #309–#363 |



| Estimated surface | ~45–70% client-visible; remaining work is Phase U through Z (including M209/M210) |



## Recent Progress



- **M156 in flight** — `information_schema.COLUMNS` adds `COLUMN_DEFAULT`, `EXTRA`, `COLUMN_KEY`; uppercase `COLUMNS` alias; projection/WHERE via `finish_information_schema_select` (#308)

- **M155 merged** — RENAME TABLE multi-pair atomic swap; mysql-diff `rename_table_multi` (#307 / PR #405)

- **M154 merged** — CREATE TABLE LIKE / AS SELECT; mysql-diff `create_table_like_ctas` (#306 / PR #404)



## Sensors



```bash

cargo fmt --all -- --check

cargo clippy --all-targets --all-features -- -D warnings

cargo test -- --skip release_binary

node scripts/harness-validate.mjs

```



## Notes



- M156: `COLUMN_DEFAULT` empty cell = SQL NULL; string literals pinned as stored expression (e.g. `'x'`). `EXTRA`: `auto_increment`, `VIRTUAL GENERATED` (M146). `COLUMN_KEY`: PRI/UNI/MUL from PK, unique index registry, secondary indexes, FK columns. Not full MySQL `COLUMNS` (charset name, `GENERATION_EXPRESSION`, etc.).

- M155: sqlparser 0.53 lacks standalone `RENAME TABLE`; `rusql-sql` parses pairs + pending rewrite to `ALTER TABLE … RENAME TO`. Storage `rename_tables` batch-validates then applies (swap-safe). errno 1050 via `ExecError::Mysql` for name conflicts.

- Phase T exit (mysqldump simple schema) still open after M156 slice.


