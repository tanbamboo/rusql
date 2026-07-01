## Summary

<!-- What does this PR do? Link spec: docs/specs/... or Closes #N -->

## Spec alignment

- [ ] Acceptance criteria met
- [ ] Changes within spec file boundaries

## Sensors

- [ ] `cargo fmt --all -- --check` pass
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` pass
- [ ] `cargo test` pass
- [ ] `node scripts/harness-validate.mjs` pass
- [ ] `node scripts/doc-parity.mjs` pass (if user-guide changed)
- [ ] CI passed on **first push** (if not, note fix in PR body)

## Documentation

- [ ] Docs updated OR no-docs-impact noted below
- [ ] **CHANGELOG.md** `[Unreleased]` updated (or N/A for harness-only internal change)
- [ ] **docs/en/release-notes.md** and **docs/zh-CN/release-notes.md** updated for user-visible changes
- [ ] `node scripts/check-changelog.mjs` pass
- [ ] i18n keys added to en-US.yml and zh-CN.yml (if user-visible strings)

## Harness

- [ ] If fixing a repeat agent failure, HARNESS_CHANGELOG.md updated

## No-docs-impact rationale

<!-- If no doc changes, explain why -->
