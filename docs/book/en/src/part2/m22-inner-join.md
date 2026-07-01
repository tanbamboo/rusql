# M22 — INNER JOIN

**Issue #45**

## Problem

Relational data is normalized across tables. Applications need combined rows (`orders` + `order_items`) via `INNER JOIN ... ON`.

## Decision

- Nested-loop join for two base tables, single `INNER JOIN ... ON col = col`.
- Combined column list = left columns then right columns.
- Post-join `WHERE` / projection / ORDER BY / LIMIT reuse M14–M21 pipeline.

## Harness lesson

> `inner_join` compat suite with multi-row match on one key exercises join + WHERE together.
