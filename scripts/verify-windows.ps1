#Requires -Version 5.1
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
Push-Location $projectRoot
try {
    if (-not (Test-Path 'dist/Cutline/tools/ffmpeg.exe')) { throw 'Run scripts/build-windows.ps1 first.' }
    $env:PATH = (Join-Path $projectRoot 'dist/Cutline/tools') + ';' + $env:PATH
    & cargo fmt --check
    if ($LASTEXITCODE -ne 0) { throw 'Formatting failed.' }
    & cargo clippy --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed.' }
    & cargo test --no-run --locked
    if ($LASTEXITCODE -ne 0) { throw 'Test compilation failed.' }
    # Test binaries resolve their own sibling media tools; application code does not trust PATH on Windows.
    New-Item -ItemType Directory -Force 'target/debug/deps/tools' | Out-Null
    Copy-Item 'dist/Cutline/tools/*.exe' 'target/debug/deps/tools/' -Force
    & cargo test --locked -- --test-threads=2
    if ($LASTEXITCODE -ne 0) { throw 'Media tests failed.' }
    Write-Host 'Automated checks passed. Complete the device checklist in docs/VERIFICATION.md.'
} finally { Pop-Location }
