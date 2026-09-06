$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$release = Join-Path $root 'release'
New-Item -ItemType Directory -Force -Path $release | Out-Null

Get-Process | Where-Object { $_.Name -match 'Mouse Insight|mouse_insight' } | ForEach-Object {
    Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
}
Start-Sleep -Milliseconds 500

$unins = @(
    "$env:LOCALAPPDATA\Programs\Mouse Insight\uninstall.exe",
    "$env:LOCALAPPDATA\Mouse Insight\uninstall.exe"
)
foreach ($u in $unins) {
    if (Test-Path $u) {
        Start-Process -FilePath $u -ArgumentList '/S' -Wait -ErrorAction SilentlyContinue
    }
}

Get-ChildItem $release -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne 'config.json' } | ForEach-Object {
    [System.IO.File]::Delete($_.FullName)
}

npm run tauri:build
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

$bundle = 'G:\build_cache\cargo-target\release\bundle'
$copied = 0
if (Test-Path $bundle) {
    Get-ChildItem $bundle -Recurse -Include *.exe, *.msi |
        Where-Object { $_.Name -like 'Mouse Insight*' } |
        ForEach-Object {
            Copy-Item $_.FullName (Join-Path $release $_.Name) -Force
            $copied++
        }
}
$portable = 'G:\build_cache\cargo-target\release\Mouse Insight.exe'
if (Test-Path $portable) {
    Copy-Item $portable (Join-Path $release 'Mouse Insight.exe') -Force
    $copied++
}

Write-Host "release files:"
Get-ChildItem $release | Format-Table Name, Length, LastWriteTime
if ($copied -lt 1) { throw "no artifacts copied into release/" }
