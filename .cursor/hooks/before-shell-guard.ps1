# Block dangerous shell commands
$raw = [Console]::In.ReadToEnd()
try { $input = $raw | ConvertFrom-Json } catch { $input = @{ command = $raw } }
$command = $input.command
if (-not $command) { Write-Output '{"permission":"allow"}'; exit 0 }

$blocked = @(
  'git push --force',
  'git push -f',
  'rm -rf',
  'Remove-Item -Recurse -Force',
  'git reset --hard',
  'git clean -fdx',
  '--no-verify'
)

foreach ($pattern in $blocked) {
  if ($command -match [regex]::Escape($pattern) -or $command -match $pattern) {
    $resp = @{
      permission = 'deny'
      user_message = "Blocked dangerous command: $pattern"
      agent_message = "This command is blocked by harness policy. Ask the human if truly needed."
    }
    $resp | ConvertTo-Json -Compress
    exit 2
  }
}

# Secret-like patterns in echo/write commands
if ($command -match '(api[_-]?key|secret|password|token)\s*=\s*[''"][^''"]+[''"]') {
  $resp = @{
    permission = 'deny'
    user_message = 'Possible secret in command — use environment variables instead.'
    agent_message = 'See .agents/guardrails/secret-policy.md'
  }
  $resp | ConvertTo-Json -Compress
  exit 2
}

Write-Output '{"permission":"allow"}'
exit 0
