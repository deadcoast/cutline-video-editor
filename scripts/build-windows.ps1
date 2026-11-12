#Requires -Version 5.1
[CmdletBinding()]
param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$projectRoot = Split-Path $PSScriptRoot -Parent
Push-Location $projectRoot
try {
    if ($PSVersionTable.PSVersion.Major -ge 6 -and -not $IsWindows) { throw 'Run this packaging script on Windows.' }
    if (-not $SkipBuild) {
        & cargo build --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'Rust release build failed.' }
    }
    $exe = Join-Path $projectRoot 'target/release/cutline.exe'
    if (-not (Test-Path $exe)) { throw 'The release executable was not found.' }
    $seven = Get-Command 7z.exe -ErrorAction SilentlyContinue
    $sevenPath = if ($seven) { $seven.Source } else { Join-Path $env:ProgramFiles '7-Zip/7z.exe' }
    if (-not (Test-Path $sevenPath)) { throw 'Install 7-Zip (https://7-zip.org), then run this script again.' }
    $lock = Get-Content 'tools.lock.json' -Raw | ConvertFrom-Json
    $vendor = Join-Path $projectRoot 'vendor'
    $distRoot = Join-Path $projectRoot 'dist'
    $stage = Join-Path $distRoot 'Cutline'
    if (Test-Path $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $vendor, $stage, (Join-Path $stage 'tools'), (Join-Path $stage 'licenses') | Out-Null
    foreach ($name in @('mpv','ffmpeg','vulkan')) {
        $entry = $lock.$name
        $archive = Join-Path $vendor $entry.archive
        if (-not (Test-Path $archive)) { Invoke-WebRequest -Uri $entry.url -OutFile $archive -UseBasicParsing }
        $actual = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $entry.sha256) { throw "Checksum mismatch for $name. Remove the cached archive and retry; do not bypass this check." }
        $unpack = Join-Path $vendor $name
        New-Item -ItemType Directory -Force -Path $unpack | Out-Null
        if ($name -eq 'mpv') {
            & $sevenPath x -y "-o$unpack" $archive | Out-Null
            if ($LASTEXITCODE -ne 0) { throw 'mpv extraction failed.' }
        } else { Expand-Archive -LiteralPath $archive -DestinationPath $unpack -Force }
    }
    Copy-Item $exe (Join-Path $stage 'Cutline.exe') -Force
    $mpv = Get-ChildItem (Join-Path $vendor 'mpv') -Recurse -Filter 'libmpv-2.dll' | Select-Object -First 1
    if (-not $mpv) { throw 'The mpv archive did not contain libmpv-2.dll.' }
    Copy-Item $mpv.FullName (Join-Path $stage 'libmpv-2.dll') -Force
    $vulkan = Get-ChildItem (Join-Path $vendor 'vulkan') -Recurse -Filter 'vulkan-1.dll' | Where-Object { $_.Directory.Name -eq 'x64' } | Select-Object -First 1
    if (-not $vulkan) { throw 'The Vulkan runtime archive did not contain the x64 loader.' }
    Copy-Item $vulkan.FullName (Join-Path $stage 'vulkan-1.dll') -Force
    $vulkanLicense = Get-ChildItem (Join-Path $vendor 'vulkan') -Recurse -Filter 'VulkanRT-License.txt' | Select-Object -First 1
    Copy-Item $vulkanLicense.FullName (Join-Path $stage 'licenses/VulkanRT-License.txt') -Force
    foreach ($name in @('ffmpeg.exe','ffprobe.exe')) {
        $tool = Get-ChildItem (Join-Path $vendor 'ffmpeg') -Recurse -Filter $name | Select-Object -First 1
        if (-not $tool) { throw "The FFmpeg archive did not contain $name." }
        Copy-Item $tool.FullName (Join-Path $stage "tools/$name") -Force
    }
    foreach ($file in @('README.md','LICENSE','THIRD_PARTY.md','tools.lock.json')) { Copy-Item $file $stage -Force }
    Copy-Item 'docs/VERIFICATION.md' $stage -Force
    $ffLicense = Get-ChildItem (Join-Path $vendor 'ffmpeg') -Recurse -Filter 'LICENSE' | Select-Object -First 1
    Copy-Item $ffLicense.FullName (Join-Path $stage 'licenses/FFmpeg-GPL.txt') -Force
    Copy-Item 'LICENSE' (Join-Path $stage 'licenses/mpv-GPL.txt') -Force
    if (Test-Path 'licenses') { Copy-Item 'licenses/*' (Join-Path $stage 'licenses') -Recurse -Force }
    $hashes = Get-ChildItem $stage -Recurse -File | Sort-Object FullName | ForEach-Object {
        $relative = $_.FullName.Substring($stage.Length + 1).Replace('\','/')
        '{0}  {1}' -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $relative
    }
    $hashes | Set-Content (Join-Path $stage 'SHA256SUMS.txt') -Encoding ascii
    $zip = Join-Path $distRoot 'Cutline-Windows-x64.zip'
    Compress-Archive -Path $stage -DestinationPath $zip -Force -CompressionLevel Optimal
    Write-Host "Ready: $zip"
} finally { Pop-Location }
