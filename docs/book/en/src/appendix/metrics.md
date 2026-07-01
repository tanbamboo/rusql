# Metrics snapshot

From [harness-retrospective-2026-06-30](../../../en/reports/harness-retrospective-2026-06-30.md) (M0–M8 window; extended delivery through M13 on same harness model).

| Metric | Value |
|--------|-------|
| Milestones M0–M13 | 14 shipped on `main` |
| PR first-pass CI | ~87.5% |
| Branch fix / rework | ~12.5% (mostly rustfmt) |
| Median PR net LOC | ~500 |
| User-filed post-merge bugs | 0 in retrospective window |
| Rust test functions | 50+ (growing with compat) |

## Interpretation

High merge cadence with low rework indicates **feedforward** (issues + ADRs) and **feedback** (compat + CI) are balanced. Main repeatable failure: **formatting** — cheap to fix.

## Living metrics

Run `node scripts/metrics.mjs` for a current JSON snapshot when updating this appendix.
