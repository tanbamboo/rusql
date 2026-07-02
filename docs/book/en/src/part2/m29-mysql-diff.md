# M29 — mysql-diff runner

**Issue #52**

## Problem

`basic.json` compat tests only exercise rusql wire behavior. The [harness retrospective](../../../en/reports/harness-retrospective-2026-06-30.md) called for **differential feedback** against real MySQL 8.0.

## Decision

- `scripts/mysql-diff.mjs` builds `rusql-server`, starts Docker `mysql:8.0`, and diffs batch (`mysql -B`) output for each step in `compat/mysql-diff.json`.
- Portable fixture subset only (no `USE rusql`, `information_schema`, or rusql-only DDL).
- Skips with exit 0 when Docker or `mysql` client is missing; CI job `mysql-diff` on ubuntu-latest installs client and runs the script.
- Documented gaps: full `basic.json` is not MySQL-comparable row-for-row; use `mysql-diff.json` for differential signal.

## Harness lesson

> Per-suite fresh rusql data dir + isolated MySQL database avoids cross-suite table leakage when diffing sequential DDL/DML.
