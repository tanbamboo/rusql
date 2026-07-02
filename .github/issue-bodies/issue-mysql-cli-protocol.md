## Problem

The official MySQL 8.0 CLI client (including `mysql:8.0` via Docker connecting to `host.docker.internal`) does not behave the same as rusql's internal wire test client (`test_support::WireClient`).

`cargo test -p rusql-server compat` passes (UPDATE, DELETE, cross-connection persistence), but differential testing with the real `mysql` client fails.

## Reproduction

```bash
cargo build -p rusql-server
# Start rusql-server on 3307 with a temp data dir, then:

docker run --rm mysql:8.0 mysql -h host.docker.internal -P 3307 -u root --protocol=TCP \
  -e "CREATE TABLE t (id INT); INSERT INTO t VALUES (1);"

docker run --rm mysql:8.0 mysql -h host.docker.internal -P 3307 -u root --protocol=TCP -B \
  -e "SELECT * FROM t"
# Expected: row 1. Often empty on rusql.

docker run --rm mysql:8.0 mysql -h host.docker.internal -P 3307 -u root --protocol=TCP \
  -e "UPDATE t SET id=2 WHERE id=1"
# ERROR 1105: unsupported statement: Update { ... }
```

Internal wire client test `connection::tests::update_across_connections` passes.

## Impact

- `scripts/mysql-diff.mjs` differential signal is unreliable when using Docker `mysql` as rusql client (7/13 steps matched in `database-compat-report-2026-06-30.md`)
- Real-world MySQL clients (CLI, some drivers) may hit the same handshake/protocol path

## Hypothesis

Handshake or caching_sha2 auth exchange with official clients may leave the connection in a state where:
- INSERT OK packets are returned but rows do not persist across connections
- UPDATE/DELETE reach executor but are rejected (1105) despite being implemented

## Acceptance criteria

- [ ] Official `mysql` 8.0 CLI: INSERT persists across separate TCP connections
- [ ] Official `mysql` 8.0 CLI: UPDATE and DELETE work over COM_QUERY
- [ ] `node scripts/mysql-diff.mjs` passes portable fixture on Windows + Docker Desktop
- [ ] `connection::tests::update_across_connections` still passes (regression guard)

## References

- Report: `docs/en/reports/database-compat-report-2026-06-30.md`
- Harness note: harness-retrospective §10 (M29 differential gaps)
