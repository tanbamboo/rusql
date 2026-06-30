#!/usr/bin/env bash
set -euo pipefail

PROFILE="typescript"
PROJECT_NAME="my-product"
VALID_PROFILES="typescript python dotnet java golang rust"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile) PROFILE="$2"; shift 2 ;;
    --project-name) PROJECT_NAME="$2"; shift 2 ;;
    *) echo "Unknown arg: $1"; exit 1 ;;
  esac
done

if ! echo "$VALID_PROFILES" | grep -qw "$PROFILE"; then
  echo "Invalid profile: $PROFILE (valid: $VALID_PROFILES)"
  exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== AI-Native Harness Bootstrap ==="
echo "Profile: $PROFILE"
echo "Project: $PROJECT_NAME"

sed -i.bak "s/^active_profile:.*/active_profile: $PROFILE/" anr.yaml && rm -f anr.yaml.bak
echo "Updated anr.yaml"

node scripts/sync-cursor-from-agents.mjs
pnpm install
if [[ "$PROFILE" == "python" ]] || [[ -d packages/api ]]; then
  pip install -e "packages/api[dev]" || true
fi
node scripts/harness-validate.mjs

echo ""
echo "=== Bootstrap complete ==="
echo "Next: update AGENTS.md for $PROJECT_NAME"
