# M24 — ALTER TABLE ADD COLUMN

**Issue #47**

## Problem

Schema evolution is the norm in production: migrations add columns without recreating tables. MySQL clients and ORMs issue `ALTER TABLE … ADD COLUMN` (or the shorthand `ADD col type`). Without it, rusql cannot run incremental migrations.

## Decision

- Parse `ALTER TABLE t ADD [COLUMN] c TYPE` via sqlparser MySQL dialect.
- Extend storage: append column to `TableMeta`, pad existing rows with empty string (NULL sentinel per M21).
- WAL record `AddColumn` for replay on restart.
- Sync session catalog after DDL so DESCRIBE / `information_schema` stay consistent.

## Internals

1. **Executor** — `execute_alter_table` handles `AlterTableOperation::AddColumn`; reuses `column_def_from_ast` from CREATE TABLE parsing.
2. **HeapEngine** — `add_column` rejects duplicates; extends each row vector.
3. **PersistentEngine** — append WAL then mutate heap; catalog re-seeded from `table_metas()` on new connections.

## Harness lesson

> `alter_add_column` compat suite checks SELECT on new column (NULL as empty string) and DESCRIBE column list together.

## References

- MySQL 8.0 Reference: [ALTER TABLE](https://dev.mysql.com/doc/refman/8.0/en/alter-table.html)
- SQLite ALTER TABLE ADD COLUMN (simpler precedent for row padding)
