# M5 — Compat fixture suite

**Merged**: PR #15 · Issue #14

## Problem

Unit tests proved crates in isolation; we needed **executable proof** that the MySQL wire path runs real SQL scenarios end-to-end.

## Design choices

- JSON suites in `crates/rusql-server/compat/basic.json`
- Runner asserts columns, rows, affected rows over **real TCP**
- Documented in user-guide as the recommended regression path

## Trade-offs

Fixtures lag behind features unless each SQL milestone adds steps — now part of ship checklist.

## Impact

Best **feedback** investment in the project ([retrospective](../../../en/reports/harness-retrospective-2026-06-30.md) §6). Encodes user-testable contract agents can extend without writing Rust.

## Harness lesson

> Prefer **data-driven wire tests** over copying Rust integration test boilerplate for every SQL feature.

## Try it

```bash
cargo test -p rusql-server run_basic_compat_fixtures
```
