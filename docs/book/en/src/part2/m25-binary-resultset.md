# M25 — Binary resultset (COM_STMT_EXECUTE)

**Issue #48**

## Problem

Drivers using prepared statements expect **binary protocol** result rows: column metadata carries MySQL `enum_field_types`, and `COM_STMT_EXECUTE` returns rows with a null bitmap plus type-specific binary values (not lenenc text). Without this, JDBC/ORM clients may mis-decode integers or reject the wire format.

## Decision

- Map catalog SQL types to MySQL wire types (`INT` → `MYSQL_TYPE_LONG`, `VARCHAR` → `MYSQL_TYPE_VAR_STRING`).
- `COM_STMT_PREPARE` column/param definitions include the mapped type byte.
- `COM_STMT_EXECUTE` SELECT responses use `binary_resultset` (0x00 row header, null bitmap offset 0).
- `COM_QUERY` remains text resultset (unchanged).

## Internals

- New `rusql-protocol::binary` module: encode/decode binary values and rows.
- `PreparedStatement` stores `result_column_types` inferred at prepare time.
- Test client decodes binary rows for wire integration tests.

## Harness lesson

> `stmt_prepare_execute_binary_table_select` locks INT + VARCHAR binary roundtrip over TCP.

## References

- [MySQL Binary Protocol Resultset](https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_binary_resultset.html)
