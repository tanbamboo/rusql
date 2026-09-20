# mysql-test skip list (M30)

Oracle **mysql-test** contains thousands of `.test` files across storage engines, replication, privileges, and SQL edge cases. M60 ports a **100-case wire subset** runnable via rusql's internal test client (`cargo test -p rusql-server mysql_test_subset`).

Portable SQL is extracted with `scripts/extract-mtr-sql.mjs`; expected output is recorded against Docker MySQL 8.0 and diffed via `scripts/mysql-diff.mjs`. The official `mysql` CLI differential gate is tracked in issue #73 (resolved via protocol smoke in CI).

## SQuaLity skip taxonomy

| Category | Examples | Reason |
|----------|----------|--------|
| **Environment** | `onlyif($ENV)`, host-specific paths | Not reproducible in rusql CI |
| **Extensions** | plugins, native UDFs, `performance_schema` | Not implemented in rusql |
| **Client-dependent** | Multi-connection, `send_eval`, psql-style commands | Real `mysql` client shape differs; use `mysql-diff` oracle |
| Stored programs | Full `sp-*` / `trigger-*` mysql-test | MVP procedures/triggers/functions/events exist; full MTR dialect (IN/OUT, SIGNAL, handlers) does not |
| **Expression / aggregate** | Remaining `func_*` beyond M46/M66/M70 | Core GROUP BY / HAVING / builtins landed; DATE/JSON/UUID packs still later |
| **Subquery** | Nested/correlated edge cases | M42 IN/EXISTS/derived tables landed; remaining MTR cases stay skipped |
| Replication / binlog | `rpl-*`, GTID event 33, heartbeat | ADR / M56–M74 MVP; dump follow exists; GTID failover later |
| Charset/collation | Other than utf8mb4_unicode_ci / 0900_ai_ci | M35/M59/M62 cover those two |
| Full optimizer | `range*`, `join_cache*` | Beyond current cost planner |
| Official mysql-test runner | `mysql-test-run.pl`, 112 runner commands | Custom JSON wire harness + extractor instead |
| Multi-connection MTR | `connect`/`disconnect` blocks | `USE` / COM_INIT_DB landed (M15); MTR connection multiplexer is not the wire subset |

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
