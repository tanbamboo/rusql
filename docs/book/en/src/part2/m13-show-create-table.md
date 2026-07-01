# M13 — SHOW CREATE TABLE

**Merged**: PR #32 · Issue #31 · [spec](../../../en/specs/m13-show-create-table.md)

## Problem

Schema export and some migrations expect **`SHOW CREATE TABLE`** DDL strings.

## Design choices

- Reconstruct `CREATE TABLE` from catalog metadata
- Backtick-quoted identifiers; uppercase types in DDL
- Tables only (no views)

## Trade-offs

No engine clause, charset, or `IF NOT EXISTS` — readability over mysqldump fidelity.

## Harness lesson

> Pair **SHOW CREATE** with compat fixture asserting exact DDL string — catches catalog/type regressions.

## See also

- [m13-show-create-table.md](../../../en/specs/m13-show-create-table.md)
