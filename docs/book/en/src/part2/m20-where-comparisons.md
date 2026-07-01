# M20 — WHERE comparisons and AND

**Issue #43**

## Problem

Real queries filter with `<`, `>`, `<>`, and combine predicates with `AND`. M4 only accelerated `col = literal`; without comparisons every non-equality filter returned full scans incorrectly (no filter).

## Decision

- Parse `WHERE` into `LiteralPredicate` + `And` tree (literals only).
- Numeric compare when cell and literal parse as `i64`, else lexicographic string compare.
- Single `=` still uses index fast path when available.

## Harness lesson

> Dedicated `where_comparisons` compat suite — avoids polluting `basic_dml` row-order assumptions.
