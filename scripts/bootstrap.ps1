param(
  [ValidateSet('rust')]
  [string]$Profile = 'rust',
  [string]$ProjectName = 'rusql'
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

Write-Host "=== Rusql Harness Bootstrap ===" -ForegroundColor Cyan
Write-Host "Profile: $Profile"
Write-Host "Project: $ProjectName"

$anrPath = Join-Path $Root 'anr.yaml'
$content = Get-Content $anrPath -Raw
$content = $content -replace 'active_profile:\s*\S+', "active_profile: $Profile"
Set-Content -Path $anrPath -Value $content -NoNewline
Write-Host "Updated anr.yaml active_profile -> $Profile"

node (Join-Path $PSScriptRoot 'harness-validate.mjs')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host ''
Write-Host '=== Bootstrap complete ===' -ForegroundColor Green
Write-Host 'Next: cargo test && gh issue list --label agent-ready'
