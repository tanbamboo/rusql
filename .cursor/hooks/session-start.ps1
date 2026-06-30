# Session start hook — remind agent to read HANDOFF.md
$input = [Console]::In.ReadToEnd() | ConvertFrom-Json
$profile = "typescript"
if (Test-Path "anr.yaml") {
  $content = Get-Content "anr.yaml" -Raw
  if ($content -match 'active_profile:\s*(\S+)') { $profile = $Matches[1] }
}
$msg = "Session start: Read HANDOFF.md and profiles/$profile/guides.md before acting."
@{ additional_context = $msg } | ConvertTo-Json -Compress
exit 0
