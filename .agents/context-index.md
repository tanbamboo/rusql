# Repository Map (Context Index)

> Agent navigation. Load linked files on demand; do not read everything at once.

## Root Contracts

| File | Purpose |
|------|---------|
| [AGENTS.md](../AGENTS.md) | Entry contract, commands, trust boundaries |
| [CONSTITUTION.md](../CONSTITUTION.md) | Non-negotiable principles |
| [HANDOFF.md](../HANDOFF.md) | Cross-session state |
| [HARNESS_CHANGELOG.md](../HARNESS_CHANGELOG.md) | Failure → fix log |
| [anr.yaml](../anr.yaml) | Machine-readable manifest |

## Documentation

| Path | Content |
|------|---------|
| [docs/en/architecture/overview.md](../docs/en/architecture/overview.md) | Architecture overview |
| [docs/en/architecture/boundaries.md](../docs/en/architecture/boundaries.md) | Crate boundaries |
| [docs/en/agent-governance/trust.md](../docs/en/agent-governance/trust.md) | Autonomy matrix |
| [docs/en/workflows/spec-to-ship.md](../docs/en/workflows/spec-to-ship.md) | Delivery workflow |
| [docs/zh-CN/](../docs/zh-CN/) | Simplified Chinese mirrors |

## Portable Agent Layer

| Path | Content |
|------|---------|
| [workflows/bootstrap.md](workflows/bootstrap.md) | Project bootstrap |
| [workflows/pr-review.md](workflows/pr-review.md) | PR review flow |
| [guardrails/protected-paths.md](guardrails/protected-paths.md) | Protected paths |
| [guardrails/secret-policy.md](guardrails/secret-policy.md) | Secret policy |
| [skills/](skills/) | Canonical skills |

## Stack Profile

| Profile | Path | Used for |
|---------|------|----------|
| rust | [profiles/rust/](../profiles/rust/) | All crates in workspace |

## Crates

| Crate | Path | Responsibility |
|-------|------|----------------|
| rusql-i18n | [crates/rusql-i18n/](../crates/rusql-i18n/) | User-visible messages |
| rusql-protocol | [crates/rusql-protocol/](../crates/rusql-protocol/) | MySQL wire protocol |
| rusql-sql | [crates/rusql-sql/](../crates/rusql-sql/) | SQL parse & AST |
| rusql-core | [crates/rusql-core/](../crates/rusql-core/) | Catalog, session |
| rusql-storage | [crates/rusql-storage/](../crates/rusql-storage/) | Storage engines |
| rusql-executor | [crates/rusql-executor/](../crates/rusql-executor/) | Query execution |
| rusql-planner | [crates/rusql-planner/](../crates/rusql-planner/) | Query planning |
| rusql-server | [crates/rusql-server/](../crates/rusql-server/) | TCP server |
| rusql-cli | [crates/rusql-cli/](../crates/rusql-cli/) | Admin CLI |

## Cursor Layer (thin wrapper)

| Path | Content |
|------|---------|
| `.cursor/rules/` | IDE rules |
| `.cursor/skills/` | Synced from `.agents/skills` |
| `.cursor/hooks/` | Lifecycle hooks |
| `.cursor/agents/` | Subagent definitions |

## Scripts

| Script | Purpose |
|--------|---------|
| `scripts/bootstrap.ps1` | One-shot bootstrap |
| `scripts/harness-validate.mjs` | Validate harness structure |
