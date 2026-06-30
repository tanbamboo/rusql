# Spec → Plan → Implement → Verify → Ship

Standard delivery pipeline for AI-native development.

## Phase 1: Spec

**Input**: User requirement, GitHub issue  
**Output**: `docs/specs/<feature>.md` or issue body

Spec must include:
- [ ] Goal (one sentence)
- [ ] Acceptance criteria (testable)
- [ ] File boundaries (allowed paths)
- [ ] Negative constraints
- [ ] Dependencies and risks

## Phase 2: Plan

**Input**: Spec  
**Output**: Implementation plan (steps, files, test strategy)

Human confirmation required before Implement for large changes.

## Phase 3: Implement

**Input**: Confirmed plan  
**Output**: Code + tests + doc updates

Constraints:
- Only modify files within spec boundaries
- Follow `profiles/rust/guides.md`
- Use `rusql-i18n` for user-visible strings

## Phase 4: Verify

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
```

## Phase 5: Ship

- Open PR with `.github/PULL_REQUEST_TEMPLATE.md`
- Link issue (`Closes #N`)
- Human review focuses on spec gaps and trade-offs
- Update HANDOFF.md after merge

## Issue Loop

1. Poll `gh issue list --label agent-ready`
2. Pick highest `priority:P0`
3. Execute spec-to-ship
4. Remove `agent-ready`, add `needs-review`
