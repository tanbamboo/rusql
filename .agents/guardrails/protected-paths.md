# Protected Paths

Paths constrained by [risk-tiers.md](../../docs/en/agent-governance/risk-tiers.md).

## T0 — Agent must not modify autonomously

```
.github/workflows/
.github/CODEOWNERS
CONSTITUTION.md
.agents/guardrails/secret-policy.md
anr.yaml                    # structural changes need human approval
```

## T1 — Editable, human review required

```
scripts/
profiles/_base/
crates/rusql-protocol/      # wire compatibility
crates/rusql-storage/       # data integrity
```

## Protection Mechanisms

1. `.cursor/rules/risk-tiers.mdc` — warnings on edit
2. `.cursor/hooks/before-shell-guard.ps1` — blocks dangerous git ops
3. `CODEOWNERS` — GitHub required review
4. CI `harness-validate` — structure integrity
