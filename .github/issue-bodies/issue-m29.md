## Goal

Run same SQL against rusql + Docker MySQL; diff results (harness feedback).

## Depends on

- M5 compat fixtures

## Acceptance Criteria

- [ ] `scripts/mysql-diff.mjs` runs when Docker available
- [ ] CI optional job; SKIP without Docker
- [ ] Document in roadmap retrospective gaps

## File Boundaries

- scripts/**, profiles/rust/sensors.yaml
