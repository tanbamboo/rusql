# Check GitHub Issue Replies

Poll `needs-human` issues for maintainer replies and surface actionable decisions.

## Usage

```bash
node scripts/check-issue-replies.mjs
```

Optional: `RUSQL_GITHUB_REPO=owner/repo`

## Agent workflow

1. Run at **every session start** (before picking `agent-ready` tasks)
2. For each issue in `withReplies`:
   - Read full thread: `gh issue view <N> --comments`
   - Record decision in `docs/en/specs/` (+ `docs/zh-CN/` mirror)
   - Comment on issue acknowledging decision
   - Close issue or remove `needs-human`; create follow-up issues if needed
3. Update `HANDOFF.md` open questions section

## Output

JSON with:
- `withReplies` — issues that have comments (decisions may be ready)
- `awaitingReply` — still waiting on human input
