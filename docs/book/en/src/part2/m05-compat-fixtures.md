# M5 — Compat fixture suite

**Merged**: PR #15 · Issue #14

## Problem

Unit tests proved individual crates in isolation, but regressions still slipped through at the **MySQL wire boundary**: handshake OK, `COM_QUERY` framing wrong, column counts mismatched. We needed executable proof that real TCP clients see correct tabular results.

Manual `mysql` CLI sessions do not scale in CI or for autonomous agents.

## Design space

| Approach | Pros | Cons |
|----------|------|------|
| JSON wire fixtures | Declarative; agents extend without Rust | Must keep in sync with features |
| Rust integration tests only | Type-safe | Duplicated boilerplate per scenario |
| External mysqltest fork | MySQL-native | Heavy dependency |

## Decision

- JSON suites in `crates/rusql-server/compat/basic.json`
- Runner asserts columns, rows, affected rows over **real TCP** (`compat_suite` tests)
- Documented in user-guide as the recommended regression path
- Each SQL milestone adds steps to existing suites where possible

## Internals

```
basic.json → compat runner → TCP :port → handshake → COM_QUERY per step → assert JSON expect
```

Suites are isolated by scenario name; steps run sequentially on one connection (session state carries).

## Trade-offs

Fixtures lag behind features unless each SQL milestone adds steps — now part of ship checklist (issue-loop rule).

## Impact

Best **feedback** investment in the project ([retrospective](../../../en/reports/harness-retrospective-2026-06-30.md) §6). Encodes a user-testable contract agents can extend without writing Rust.

## Further reading

- MySQL Internals: [Protocol basics](https://dev.mysql.com/doc/internals/en/client-server-protocol.html)
- Harness retrospective — feedforward vs feedback loops

## Harness lesson

> Prefer **data-driven wire tests** over copying Rust integration test boilerplate for every SQL feature.

## Try it

```bash
cargo test -p rusql-server run_basic_compat_fixtures
```
