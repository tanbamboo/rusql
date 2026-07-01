# M16 — SELECT LIMIT

**Issue #38**

## Problem

ORMs and APIs paginate with `LIMIT`; without it every query returns full scans.

## Design choices

- Read `Query.limit` from sqlparser AST
- Apply `take(n)` **after** scan, projection, and filter (simple MVP)

## Trade-offs

No `OFFSET`, no optimizer pushdown — acceptable for small heaps.

## Harness lesson

> Pagination tests belong in **compat JSON** next to existing `basic_dml` inserts.
