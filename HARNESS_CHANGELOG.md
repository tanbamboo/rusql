# HARNESS_CHANGELOG — Failure → Fix Log

Record repeated agent failures and the harness changes that prevent recurrence.

Format:

```markdown
## YYYY-MM-DD — Short title

**Failure pattern**: What the agent kept doing wrong
**Fix**: Guide / sensor / rule added
**Files**: paths changed
```

---

## 2026-09-20 — HANDOFF lagged M107; SHOW EVENTS last-executed myth

**Failure pattern**: Feature landed on a branch with CHANGELOG/user-guide updated, but HANDOFF still said "implement M107"; leftover CI issue drafts from a red `main` were never deleted; docs claimed last-executed belongs on `SHOW EVENTS` (MySQL 8.0 has 15 columns; last-executed is `information_schema.EVENTS`).
**Fix**: Session protocol: after a feature commit, refresh HANDOFF before opening the PR. Gap probe `scripts/mysql-gap-probe.mjs` records empirical failures. Do not file a 16th `SHOW EVENTS` column.
**Files**: HANDOFF.md, scripts/mysql-gap-probe.mjs, crates/rusql-server/compat/mysql-gap-probe.json, docs/en/specs/mysql-full-parity-roadmap.md

**Failure pattern**: N/A (initial harness)
**Fix**: Bootstrapped rusql harness from ai-native-harness-template (Rust-only, issue-driven loop)
**Files**: AGENTS.md, anr.yaml, profiles/rust/, .cursor/rules/, crates/
