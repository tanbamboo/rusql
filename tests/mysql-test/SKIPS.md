# mysql-test skip list (M30)

Oracle **mysql-test** contains thousands of `.test` files across storage engines, replication, privileges, and SQL edge cases. M60 ports a **100-case wire subset** runnable via rusql's internal test client (`cargo test -p rusql-server mysql_test_subset`).

Portable SQL is extracted with `scripts/extract-mtr-sql.mjs`; expected output is recorded against Docker MySQL 8.0 and diffed via `scripts/mysql-diff.mjs`. The official `mysql` CLI differential gate is tracked in issue #73 (resolved via protocol smoke in CI).

## SQuaLity skip taxonomy

| Category | Examples | Reason |
|----------|----------|--------|
| **Environment** | `onlyif($ENV)`, host-specific paths | Not reproducible in rusql CI |
| **Extensions** | `sp-*`, `trigger-*`, UDF, plugins | Not implemented in rusql |
| **Client-dependent** | Multi-connection, `send_eval`, psql-style commands | Real `mysql` client shape differs; use `mysql-diff` oracle |
| Stored programs | `sp-*`, `trigger-*` | No procedures/triggers in rusql |
| **Expression / aggregate** | `func_*`, `group_by_*`, `having_*` | M43/M46 — enable when executor supports |
| **Subquery** | `subselect_*`, derived tables | M42 — enable when IN/EXISTS/derived stable |
| Replication / binlog | `rpl-*`, `binlog-*` | ADR / M34 |
| Charset/collation | `ctype-*`, utf8mb4 metadata | M35 |
| Full optimizer | `range*`, `join_cache*` | Beyond MVP executor |
| Official mysql-test runner | `mysql-test-run.pl`, 112 runner commands | Custom JSON wire harness + extractor instead |
| Multi-database | `connection` commands | Blocked until COM_INIT_DB (#77) |

## Extraction rules (`extract-mtr-sql.mjs`)

1. Keep: `SELECT`, `INSERT`, `CREATE TABLE`, `UPDATE`, `DELETE`, `SHOW`, `DESCRIBE`, transactions
2. Drop: `onlyif`/`skipif` blocks, loops, file I/O, `connect`/`disconnect`, `eval_result`
3. Tag each suite with `origin: mysql-test/t/foo.test`
4. Record expectations via WireClient first; refine with `mysql-diff --record` when semantics match MySQL 8.0

## Running the subset

```bash
node scripts/mysql-test-subset.mjs
# or
cargo test -p rusql-server mysql_test_subset

# Protocol smoke (official mysql client only)
node scripts/mysql-diff.mjs --smoke-only

# Extract portable SQL from a donor .test file
node scripts/extract-mtr-sql.mjs --name my_case path/to/foo.test
```
