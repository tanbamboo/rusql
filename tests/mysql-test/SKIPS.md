# mysql-test skip list (M30)

Oracle **mysql-test** contains thousands of `.test` files across storage engines, replication, privileges, and SQL edge cases. M60 ports a **100-case wire subset** runnable via rusql's internal test client (`cargo test -p rusql-server mysql_test_subset`).

Portable SQL is extracted with `scripts/extract-mtr-sql.mjs`; expected output is recorded against Docker MySQL 8.0 and diffed via `scripts/mysql-diff.mjs`. The official `mysql` CLI differential gate is tracked in issue #73 (resolved via protocol smoke in CI).

## SQuaLity skip taxonomy

| Category | Examples | Reason |
|----------|----------|--------|
| **Environment** | `onlyif($ENV)`, host-specific paths | Not reproducible in rusql CI |
| **Extensions** | plugins, native UDFs, `performance_schema` | MTR suites outside the 100-case subset. Enterprise plugins are the M188 **out of scope** list below. `performance_schema` remains M189. Native UDFs remain M201 |
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

## Enterprise plugins (M188) — out of scope

Oracle MySQL Enterprise Edition plugins below are **out of scope**. They are closed-source and are not part of MySQL 8.0 Community Server. rusql does not load them and does not report them as loaded. The same list is in [docs/en/reports/rusql-vs-mysql.md](../../docs/en/reports/rusql-vs-mysql.md) and [docs/zh-CN/reports/rusql-vs-mysql.md](../../docs/zh-CN/reports/rusql-vs-mysql.md). ADR: [adr-m188-enterprise-plugins.md](../../docs/en/architecture/adr-m188-enterprise-plugins.md). This skip does not mark the MySQL 8.0 goal complete.

mysql-test suites that install or assert these plugins stay skipped. Required names: audit, firewall, data masking, thread pool plugin.

| Plugin / component | Product |
|--------------------|---------|
| `audit_log` | MySQL Enterprise Audit (not rusql `--audit-log`) |
| `mysql_firewall` | MySQL Enterprise Firewall |
| `data_masking` | MySQL Enterprise Data Masking and De-identification |
| `component_masking` | Data masking component (`component_masking_functions` included) |
| `thread_pool` | MySQL Enterprise Thread Pool plugin |
| `authentication_pam` | Enterprise PAM authentication |
| `authentication_ldap_sasl` | Enterprise LDAP (SASL) |
| `authentication_ldap_simple` | Enterprise LDAP (simple bind) |
| `authentication_windows` | Enterprise Windows native authentication |
| `keyring_okv` | Oracle Key Vault keyring |
| `keyring_aws` | AWS keyring |
| `keyring_hashicorp` | HashiCorp Vault keyring |
| `keyring_oci` | OCI Vault keyring |
| `enterprise_encryption` | MySQL Enterprise Encryption |
| `version_tokens` | Enterprise version tokens |

Still in scope (not this skip):

- `performance_schema` (M189), slow query log (M191), general log (M192), error log (M195)
- `validate_password` (M183, community, built-in)
- `caching_sha2_password`, `mysql_native_password`
- `rpl_semi_sync_master` (M178, community semisync)
- `keyring_file` (community keyring)
- `rewriter`, `connection_control` (community)
- `group_replication` (M199), `clone` (M200)
- UDF `.so` (M201) and `INSTALL PLUGIN` (M202) — separate issues; not a successful load

The **Extensions** row above is the M30 extractor taxonomy. It is not this out-of-scope list.
