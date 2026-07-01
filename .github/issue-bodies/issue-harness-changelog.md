## Goal

After every merged PR, keep user-facing **Release Notes** and developer **CHANGELOG** up to date so `main` always documents what changed and how to verify it.

## Acceptance Criteria

- [ ] `CHANGELOG.md` (Keep a Changelog format) with M0–M9 backfill + process for new entries
- [ ] `docs/en/release-notes.md` (user-friendly, test steps per milestone)
- [ ] `docs/zh-CN/release-notes.md` mirror
- [ ] PR template + `spec-to-ship.md` + `issue-loop.mdc` require changelog/release-note updates on ship
- [ ] `scripts/check-changelog.mjs` sensor validates structure and recent version section
- [ ] `anr.yaml` lists required changelog files
- [ ] `profiles/rust/sensors.yaml` includes changelog sensor in CI

## File Boundaries

- `CHANGELOG.md`
- `docs/en/release-notes.md`, `docs/zh-CN/release-notes.md`
- `docs/en/workflows/spec-to-ship.md`
- `.cursor/rules/issue-loop.mdc`
- `.github/PULL_REQUEST_TEMPLATE.md`
- `scripts/check-changelog.mjs`
- `anr.yaml`, `profiles/rust/sensors.yaml`

## Negative Constraints

- Do not duplicate full user-guide content in release notes — link to user-guide for deep how-tos
- `HARNESS_CHANGELOG.md` remains for agent failure patterns only (not product changelog)

## Manual test

1. Run `node scripts/check-changelog.mjs` — passes on main
2. Open `docs/en/release-notes.md` — each milestone has verify command
