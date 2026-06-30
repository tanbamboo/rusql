# Trust and Autonomy Matrix

## Autonomy Levels

| Level | Name | Agent behavior | Human involvement |
|-------|------|----------------|-------------------|
| L0 | Read-only | Read, analyze, suggest | All writes |
| L1 | Assisted | Edit non-critical paths, add tests | Review all PRs |
| L2 | Trusted | Merge routine PRs that pass CI | Review high-risk changes |
| L3 | Autonomous | End-to-end spec-to-ship | Exceptions and architecture only |

**Default for rusql: L1.**

## Agents May Autonomously

- Implement features in `crates/**` within issue file boundaries
- Run `cargo fmt`, `clippy`, `test`, `harness-validate`
- Update documentation related to the change
- Create feature branches and PRs linked to issues

## Agents Must Stop and Ask Humans

- Modify `CONSTITUTION.md`, `.github/CODEOWNERS`
- Delete or rename top-level directory structure
- Introduce major new external dependencies
- Breaking storage format or on-disk schema changes
- Authentication, authorization, or TLS model changes
- CI fails after 3 fix attempts

## Stop Conditions

1. Spec is ambiguous — acceptance criteria not testable
2. No access to required external systems
3. Sensor output is contradictory or unparseable
4. Suspected security vulnerability
5. Change exceeds issue file boundaries

## Related

- [Risk tiers](risk-tiers.md)
- [CONSTITUTION.md](../../CONSTITUTION.md)
