# M28 — SHOW INDEX

**Issue #51**

## Problem

MySQL clients and migration tools run `SHOW INDEX FROM tbl` to inspect indexes. M27 added `information_schema.STATISTICS`, but many drivers still issue the classic SHOW command.

## Decision

- Rewrite `SHOW INDEX` / `SHOW INDEXES` / `SHOW KEYS` in `rusql-sql` to an internal virtual table query (sqlparser 0.53 has no SHOW INDEX AST).
- Executor serves `__rusql_show_index` with MySQL column names: `Table`, `Non_unique`, `Key_name`, `Seq_in_index`, `Column_name`, `Index_type`.
- Rows come from catalog `PRIMARY KEY` metadata plus `StorageEngine::index_metas()` for secondary indexes.

## Harness lesson

> `show_index` compat suite checks PRIMARY + secondary index rows for `SHOW INDEX` and `SHOW INDEXES`.
