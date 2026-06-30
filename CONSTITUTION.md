# CONSTITUTION — Non-Negotiable Principles

All agents and human contributors must follow these rules.

## 1. Spec-First

- Every feature starts from a clear spec: goal, acceptance criteria, file boundaries, negative constraints
- Specs live in `docs/specs/` or GitHub issue bodies
- **The spec is the program** — vague prompts produce vague code

## 2. Local Gates = CI Gates

- Checks that pass locally must pass in CI
- Never skip local validation assuming CI will catch issues
- Agents must run profile sensors before declaring a task complete

## 3. Documentation Impact

Every implementation change must either:
- Update relevant `docs/` files, or
- State "no docs impact" and why in the PR

## 4. No Secrets Policy

- Never commit keys, tokens, or credentials
- Use environment variables or secret managers
- See [.agents/guardrails/secret-policy.md](.agents/guardrails/secret-policy.md)

## 5. Harness Evolution Obligation

When agents repeat the same class of mistake:
1. Record in [HARNESS_CHANGELOG.md](HARNESS_CHANGELOG.md)
2. Add a guide (feedforward) or sensor (feedback)
3. Prefer upgrading prose rules to deterministic checks

## 6. Human Review Focuses on High Value

Harness aims to focus human review on:
- Business correctness
- Architecture trade-offs
- Security and compliance boundaries

## 7. Portable Investment First

Build assets that migrate across tools:
1. MCP servers
2. AGENTS.md / `.agents/`
3. CI sensors
4. Tool-specific rules (Cursor `.mdc`, etc.)

## 8. Internationalization

- Default project language: **English** (`en-US`)
- User-visible strings in `crates/` must use `rusql-i18n` keys
- Simplified Chinese (`zh-CN`) must be supported for all user-facing messages

## 9. Issue Scope Discipline

- One PR covers one `area:*` label when possible
- Do not expand scope beyond the linked issue spec

**简体中文**: [docs/zh-CN/CONSTITUTION.md](docs/zh-CN/CONSTITUTION.md)
