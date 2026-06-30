# Harness Evolution

When agents repeat mistakes, upgrade the harness — not the prompt.

## Upgrade Path

```
Prose rule (guide) → Lint rule → CI sensor → Hook
```

## Process

1. Record failure in HARNESS_CHANGELOG.md
2. Add or update guide in `profiles/rust/guides.md`
3. Add sensor (clippy lint, test, harness-validate check)
4. Verify `node scripts/harness-validate.mjs` passes

## Monthly Audit

Run the `harness-audit` skill to check for dead rules, missing sensors, and doc rot.
