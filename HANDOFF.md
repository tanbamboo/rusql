# HANDOFF — Cross-Session State

> Update at the start and end of every agent session.

| Field | Value |
|-------|-------|
| Last updated | 2026-06-30 |
| Active profile | rust |
| Current issue | #2 — M1 MySQL handshake + OK packet (`agent-ready`) |
| Branch | main |
| Blockers | GitHub Projects API needs `gh auth refresh -s project,read:project` for board |
| Next step | Implement M1 protocol handshake per issue #2 spec |

## Recent Progress

- Harness Engineering foundation bootstrapped and pushed to `main`
- Cargo workspace: 9 crates with passing tests
- i18n: en-US default + zh-CN (`rusql-i18n`)
- GitHub: labels, milestones (M0–M6+), issues #1–#5
- Issue #1 closed (harness verification done)
- Issue #2 labeled `agent-ready` for Loop Engineering

## Open Questions (GitHub Issues)

- [#3](https://github.com/tanbamboo/rusql/issues/3): Authentication strategy (`mysql_native_password` vs `caching_sha2_password`)
- [#4](https://github.com/tanbamboo/rusql/issues/4): SQL parser choice confirmation
- [#5](https://github.com/tanbamboo/rusql/issues/5): Replication ADR draft

## Sensor Status (last run — local)

```
cargo fmt --all -- --check   OK
cargo clippy                 OK
cargo test                   OK (all crates)
harness-validate             OK
```

## Loop Engineering

Poll: `gh issue list --repo tanbamboo/rusql --label agent-ready --json number,title,labels`

Rules: `.cursor/rules/issue-loop.mdc`
