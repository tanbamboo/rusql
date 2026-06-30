# ADR: SQL Parser Strategy

**Status**: Accepted (via [#4](https://github.com/tanbamboo/rusql/issues/4#issuecomment-4840905461))  
**Date**: 2026-06-30

## Decision

Use **`sqlparser`** crate with **MySQL dialect** (Option A).

Extend or patch incrementally when `mysql-test-runner` reveals gaps (Option B targeted extensions).

## Rationale

- Already integrated in `rusql-sql`
- Fast path to basic CREATE/SELECT/INSERT
- Full custom parser is out of scope for MVP

## Consequences

- Some MySQL-specific syntax will fail until explicitly extended
- Compatibility gaps tracked via M5 compat test suite
