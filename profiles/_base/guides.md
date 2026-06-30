# Shared base guides for all profiles

## Shared Principles

1. Spec-first delivery
2. Local gates = CI gates
3. Hashimoto loop
4. One session, one task
5. i18n for user-visible strings

## Shared Sensors

| Check | Description |
|-------|-------------|
| harness-validate | Repository structure integrity |
| cargo clippy | Rust lint |
| cargo test | Test suite |

## Shared Guardrails

- [protected-paths.md](../../.agents/guardrails/protected-paths.md)
- [secret-policy.md](../../.agents/guardrails/secret-policy.md)
