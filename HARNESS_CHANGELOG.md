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

## 2026-06-30 — Project bootstrap

**Failure pattern**: N/A (initial harness)
**Fix**: Bootstrapped rusql harness from ai-native-harness-template (Rust-only, issue-driven loop)
**Files**: AGENTS.md, anr.yaml, profiles/rust/, .cursor/rules/, crates/
