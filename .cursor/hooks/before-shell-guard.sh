#!/usr/bin/env bash
input=$(cat)
command=$(echo "$input" | jq -r '.command // empty')

blocked_patterns=(
  'git push --force'
  'git push -f'
  'rm -rf'
  'git reset --hard'
  'git clean -fdx'
  '--no-verify'
)

for pattern in "${blocked_patterns[@]}"; do
  if [[ "$command" == *"$pattern"* ]]; then
    echo "{\"permission\":\"deny\",\"user_message\":\"Blocked: $pattern\",\"agent_message\":\"See harness shell guard policy.\"}"
    exit 2
  fi
done

echo '{"permission":"allow"}'
exit 0
