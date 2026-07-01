# M21 — IS NULL / IS NOT NULL

**Issue #44**

## Problem

Nullable columns are core to SQL. Without `IS NULL`, clients cannot filter missing values; `= NULL` is always unknown in SQL semantics.

## Decision

- `INSERT … NULL` stores empty string as NULL sentinel (MVP).
- `WHERE col IS NULL` / `IS NOT NULL` in predicate tree (extends M20).
- Three-valued logic for `= NULL` not implemented — use `IS NULL`.

## Harness lesson

> Test `INSERT NULL` + `IS NULL` in compat — storage and filter must agree on sentinel.
