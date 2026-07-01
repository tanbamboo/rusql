# The rusql harness stack

This chapter maps the concrete artifacts agents and humans touch every session.

## Work queue

- **GitHub Issues** with `agent-ready` and `priority:P0`
- Issue bodies: goal, acceptance criteria, **file boundaries**
- `node scripts/check-issue-replies.mjs` for `needs-human` unblock

## Specifications

| Artifact | Role |
|----------|------|
| ADRs (`docs/en/specs/adr-*.md`) | Irreversible forks (auth, parser) |
| Milestone specs (`m9-transactions.md`, …) | Testable slice for one PR |
| Issue body templates (`.github/issue-bodies/`) | Reusable scope contracts |

## Sensors (`profiles/rust/sensors.yaml`)

Fast: rustfmt, clippy. Standard: `cargo test`. CI adds harness-validate, doc-parity, changelog-check, handoff-check.

## Cross-session memory

- **HANDOFF.md** — branch, next step, recent merges
- **CHANGELOG.md** + **release-notes** — user-visible history per PR (#23 policy)

## Executable user contract

- **user-guide** (en + zh-CN) — how to verify on `main`
- **compat/basic.json** — wire-level SQL scenarios (M5+)

## Agent rules

`.cursor/rules/issue-loop.mdc` enforces: poll issues, ship docs with features, no “should I continue?” stalls.

## Design choice

We preferred **markdown in-repo** over a wiki so sensors can lint structure and PRs version docs with code.

## Trade-off

More markdown to maintain — mitigated by checklist on every merge and milestone-sized updates only.

## Harness lesson

> If an agent cannot **verify** a claim locally in &lt;2 minutes, add a sensor or fixture before adding features.
