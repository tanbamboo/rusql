# Post-edit: suggest format for TS/JS/Python files (non-blocking)
$raw = [Console]::In.ReadToEnd()
try { $input = $raw | ConvertFrom-Json } catch { exit 0 }
$path = $input.file_path
if (-not $path) { exit 0 }

$ext = [System.IO.Path]::GetExtension($path).ToLower()
switch ($ext) {
  { $_ -in '.ts', '.tsx', '.js', '.jsx' } {
  }
  '.py' {
  }
  default { exit 0 }
}

# Non-blocking context injection
@{ additional_context = "File edited: $path — run format/lint before marking task complete." } | ConvertTo-Json -Compress
exit 0
