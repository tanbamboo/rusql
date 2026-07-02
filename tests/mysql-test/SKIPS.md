# mysql-test skip list (M30)

Oracle **mysql-test** contains thousands of `.test` files across storage engines, replication, privileges, and SQL edge cases. M30 ports a **12-case wire subset** runnable via rusql's internal test client (`cargo test -p rusql-server mysql_test_subset`).

## Not ported (documented skips)

| Category | Examples | Reason |
|----------|----------|--------|
| Stored programs | `sp-*`, `trigger-*` | No procedures/triggers in rusql |
| Views | `view-*` | M33 planned |
| Replication / binlog | `rpl-*`, `binlog-*` | ADR / M34 |
| Charset/collation | `ctype-*`, utf8mb4 metadata | M35 |
| Full optimizer | `range*`, `join_cache*` | Beyond MVP executor |
| Official mysql-test runner | `mysql-test-run.pl` | Custom JSON wire harness instead |
| Official `mysql` CLI differential | — | See issue #73 |

## Running the subset

```bash
node scripts/mysql-test-subset.mjs
# or
cargo test -p rusql-server mysql_test_subset
```
