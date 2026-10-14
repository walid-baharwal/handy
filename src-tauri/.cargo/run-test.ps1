$ErrorActionPreference = "Stop"

if ($args.Count -eq 0) {
  throw "Cargo did not provide a test executable"
}

$exe = $args[0]
$testArgs = if ($args.Count -gt 1) { $args[1..($args.Count - 1)] } else { @() }
$kitBin = "C:\Program Files (x86)\Windows Kits\10\bin"
$mt = Get-ChildItem $kitBin -Directory |
  Sort-Object Name -Descending |
  ForEach-Object { Join-Path $_.FullName "x64\mt.exe" } |
  Where-Object { Test-Path $_ } |
  Select-Object -First 1

if (-not $mt) {
  throw "Windows SDK mt.exe is required to run Tauri tests"
}

$manifest = Join-Path $PSScriptRoot "comctl-v6.xml"
& $mt -nologo -manifest $manifest "-outputresource:$exe;#1" | Out-Null
if ($LASTEXITCODE -ne 0) {
  throw "Could not embed the Common Controls manifest into $exe"
}

& $exe @testArgs
exit $LASTEXITCODE
