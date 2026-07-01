# M6 — Auth and DML

**Merged**: PR #17 · Issue #16

## Problem

Dev-open servers are fine for hacking; serious demos need **`--auth-password`**, plus `DROP TABLE` and `DELETE` for DML completeness.

## Design choices

| Topic | Choice |
|-------|--------|
| Auth | Optional verify; `mysql_native_password` scramble ([adr-m6](../../../en/specs/adr-m6-auth-and-dml.md)) |
| DROP | Catalog + engine + WAL alignment |
| DELETE | Row filter by `WHERE col = literal` or full scan |

Issue #16 included an explicit **decision table** — model for human-in-the-loop without blocking agents.

## Trade-offs

Auth remains **fast-path** only until M7; no account management, no `GRANT`.

## Harness lesson

> Put **decision tables in issues** when product choices matter; agents implement the selected row.

## See also

- [adr-m6-auth-and-dml.md](../../../en/specs/adr-m6-auth-and-dml.md)
