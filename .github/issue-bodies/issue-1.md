## Goal

Verify Harness Engineering bootstrap is complete and all sensors pass on CI.

## Acceptance Criteria

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] `node scripts/harness-validate.mjs` passes
- [ ] GitHub Actions CI workflow is green on `main`

## File Boundaries

`AGENTS.md`, `anr.yaml`, `.github/workflows/`, `profiles/rust/`, `scripts/`

## Negative Constraints

- Do not add feature implementation beyond harness verification
- Do not modify CONSTITUTION.md without human approval
