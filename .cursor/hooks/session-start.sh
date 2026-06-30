#!/usr/bin/env bash
# Unix fallback for session start
input=$(cat)
profile="typescript"
if [[ -f anr.yaml ]]; then
  profile=$(grep -E '^active_profile:' anr.yaml | awk '{print $2}')
fi
echo "{\"additional_context\":\"Session start: Read HANDOFF.md and profiles/${profile}/guides.md before acting.\"}"
exit 0
