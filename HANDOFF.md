# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-06-30 |
| Branch | main |
| Next step | M22 INNER JOIN (#45) — label `agent-ready` after M21 merge |
| Roadmap | [mysql-compat-roadmap.md](docs/en/specs/mysql-compat-roadmap.md) |

## Recent Progress

- M20 merged (#63): WHERE comparisons and AND (#43)
- M19 merged (#62): LIMIT OFFSET (#42)
- M18 merged (#61): column aliases (#41)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
```
