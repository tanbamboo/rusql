# Risk Tiers

## T0 — Human approval required

- `CONSTITUTION.md`
- `.github/CODEOWNERS`
- `.github/workflows/`
- `profiles/_base/`
- `.agents/guardrails/`
- On-disk storage format changes

## T1 — Careful review

- `anr.yaml`, `AGENTS.md`
- `crates/rusql-protocol/` (wire compatibility)
- `crates/rusql-storage/` (data integrity)
- Authentication code

## T2 — Standard review

- `crates/rusql-sql/`, `rusql-executor/`, `rusql-planner/`
- Tests and documentation

## T3 — Low risk

- `crates/rusql-i18n/` locale additions
- Harness changelog entries
- HANDOFF.md updates
