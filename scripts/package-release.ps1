[CmdletBinding()]
param(
    [switch]$StopRunningApp,
    [switch]$SkipBuild,
    [switch]$CreateShortcuts
)

$ErrorActionPreference = 'Stop'
# Some hosts keep an incompatible PowerShell 7 copy of this module earlier in
# PSModulePath, which makes Get-FileHash unresolvable under Windows PowerShell.
if ($PSVersionTable.PSVersion.Major -lt 7 -and -not (Get-Command Get-FileHash -ErrorAction SilentlyContinue)) {
    Import-Module Microsoft.PowerShell.Utility -RequiredVersion 3.1.0.0
}
$root = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
Set-Location -LiteralPath $root
$releaseDir = Join-Path $root 'release'
$dataDir = Join-Path $releaseDir 'data'
$assetsDir = Join-Path $releaseDir 'assets'
$metadataDir = Join-Path $releaseDir 'metadata'
$package = Get-Content -LiteralPath (Join-Path $root 'package.json') -Raw | ConvertFrom-Json
$version = [string]$package.version
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Assert-ReleasePath([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    if (-not $full.StartsWith($releaseDir + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to modify a path outside release: $full"
    }
    return $full
}

function Write-Utf8([string]$Path, [string]$Text) {
    [IO.File]::WriteAllText($Path, $Text, $utf8)
}

# Reuse the configured Cargo target; use the existing global cache on this host.
if (-not $env:CARGO_TARGET_DIR -and (Test-Path -LiteralPath 'G:\build_cache\cargo-target')) {
    $env:CARGO_TARGET_DIR = 'G:\build_cache\cargo-target'
}
$targetDir = if ($env:CARGO_TARGET_DIR) {
    if ([IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) }
    else { [IO.Path]::GetFullPath((Join-Path $root ('src-tauri\' + $env:CARGO_TARGET_DIR))) }
} else { Join-Path $root 'src-tauri\target' }
$targetRelease = Join-Path $targetDir 'release'
$exeName = 'Mouse Insight.exe'
$setupName = "Mouse Insight_${version}_x64-setup.exe"
$builtExe = Join-Path $targetRelease $exeName
$builtSetup = Join-Path $targetRelease "bundle\nsis\$setupName"
$destExe = Join-Path $releaseDir $exeName

& node (Join-Path $PSScriptRoot 'check-release-version.mjs')
if ($LASTEXITCODE -ne 0) { throw 'Version consistency check failed.' }
if (-not $SkipBuild) {
    # Invoke the already-installed CLI directly; npm wrappers can swallow flags.
    & node (Join-Path $root 'node_modules\@tauri-apps\cli\tauri.js') build --bundles nsis
    if ($LASTEXITCODE -ne 0) { throw "Tauri build failed ($LASTEXITCODE); existing release is untouched." }
}
foreach ($artifact in @($builtExe, $builtSetup)) {
    if (-not (Test-Path -LiteralPath $artifact -PathType Leaf)) { throw "Missing package: $artifact" }
}
if ((Get-Item -LiteralPath $builtExe).VersionInfo.ProductVersion -ne $version) {
    throw 'Built executable version does not match package.json; refusing stale output.'
}

# Stage complete, version-checked artifacts before touching the active portable app.
New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null
$stageDir = Assert-ReleasePath (Join-Path $releaseDir ('.staging-' + [guid]::NewGuid().ToString('N')))
New-Item -ItemType Directory -Path $stageDir | Out-Null
Copy-Item -LiteralPath $builtExe -Destination (Join-Path $stageDir $exeName)
Copy-Item -LiteralPath $builtSetup -Destination (Join-Path $stageDir $setupName)
Copy-Item -LiteralPath (Join-Path $root 'src-tauri\icons\icon.ico') -Destination (Join-Path $stageDir 'app.ico')

# Never stop unrelated installations or processes merely sharing an app name.
$running = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
    try { $_.Path -and [string]::Equals($_.Path, $destExe, [StringComparison]::OrdinalIgnoreCase) } catch { $false }
})
if ($running.Count -and -not $StopRunningApp) {
    throw 'Portable app is running. Exit from its tray menu, or pass -StopRunningApp to replace this exact instance.'
}
if ($running.Count) {
    Write-Host 'Stopping the existing portable instance before replacing files...'
    # beta.4+ supports graceful shutdown, including releasing held mapping keys.
    $helper = Start-Process -FilePath $destExe -ArgumentList '--quit' -WindowStyle Hidden -PassThru
    foreach ($process in $running) {
        if (-not $process.WaitForExit(2500)) {
            # Older versions do not implement --quit. Replacement was explicitly requested.
            Stop-Process -Id $process.Id -Force
            $process.WaitForExit()
        }
    }
    if (-not $helper.WaitForExit(5000)) {
        # The helper may linger if the running instance ignored --quit; kill it so a
        # stale process cannot hold the lock we just replaced.
        Stop-Process -Id $helper.Id -Force -ErrorAction SilentlyContinue
        $helper.WaitForExit()
    }
}

# Backup outside release. This also preserves configuration before its relocation.
$backupDir = Join-Path $env:LOCALAPPDATA ('MouseInsight\package-backups\' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
New-Item -ItemType Directory -Path $backupDir -Force | Out-Null
$ownedRootFiles = @(Get-ChildItem -LiteralPath $releaseDir -Force -File | Where-Object {
    $_.Name -eq $exeName -or $_.Name -match '^Mouse Insight_.+_x64-setup\.exe$' -or
    $_.Name -in @('.portable', 'portable', 'app.ico', 'latest.json', 'SHA256SUMS.txt') -or
    $_.Name -match '^config\.json($|\.)'
})
foreach ($file in $ownedRootFiles) { Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $backupDir $file.Name) }
if (Test-Path -LiteralPath (Join-Path $dataDir 'config.json')) {
    Copy-Item -LiteralPath (Join-Path $dataDir 'config.json') -Destination (Join-Path $backupDir 'data-config.json')
}
foreach ($dir in @($dataDir, $assetsDir, $metadataDir)) {
    $null = Assert-ReleasePath $dir
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
}
# Do not guess which of two different configurations the user wants to retain.
$legacyConfig = Join-Path $releaseDir 'config.json'
$newConfig = Join-Path $dataDir 'config.json'
if ((Test-Path -LiteralPath $legacyConfig) -and (Test-Path -LiteralPath $newConfig)) {
    if ((Get-FileHash -LiteralPath $legacyConfig).Hash -ne (Get-FileHash -LiteralPath $newConfig).Hash) {
        throw "Conflicting root/data configurations; both preserved. Backup: $backupDir"
    }
}
foreach ($file in @($ownedRootFiles | Where-Object { $_.Name -match '^config\.json($|\.)' })) {
    $source = Assert-ReleasePath $file.FullName
    $destination = Assert-ReleasePath (Join-Path $dataDir $file.Name)
    Move-Item -LiteralPath $source -Destination $destination -Force
}
Write-Utf8 (Join-Path $dataDir '.portable') ''
Copy-Item -LiteralPath (Join-Path $stageDir $exeName) -Destination $destExe -Force
Copy-Item -LiteralPath (Join-Path $stageDir $setupName) -Destination (Join-Path $releaseDir $setupName) -Force
Copy-Item -LiteralPath (Join-Path $stageDir 'app.ico') -Destination (Join-Path $assetsDir 'app.ico') -Force

# Publish-ready copies use the dotted file names expected by the release URLs, so
# uploading never requires manual renaming.
$publishDir = Assert-ReleasePath (Join-Path $releaseDir 'publish')
New-Item -ItemType Directory -Path $publishDir -Force | Out-Null
$dotExe = "Mouse.Insight.exe"
$dotSetup = "Mouse.Insight_${version}_x64-setup.exe"
$portableZip = "Mouse.Insight_${version}_portable.zip"
Copy-Item -LiteralPath $destExe -Destination (Join-Path $publishDir $dotExe) -Force
Copy-Item -LiteralPath (Join-Path $releaseDir $setupName) -Destination (Join-Path $publishDir $dotSetup) -Force
$zipStage = Join-Path $stageDir 'portable'
New-Item -ItemType Directory -Path (Join-Path $zipStage 'data') -Force | Out-Null
Copy-Item -LiteralPath $destExe -Destination (Join-Path $zipStage $exeName)
Write-Utf8 (Join-Path $zipStage 'data\.portable') ''
Write-Utf8 (Join-Path $zipStage 'README-portable.txt') "解压后运行 Mouse Insight.exe；配置保存在同目录 data/ 下。`n"
Compress-Archive -Path (Join-Path $zipStage '*') -DestinationPath (Assert-ReleasePath (Join-Path $publishDir $portableZip)) -Force
$publishSums = foreach ($name in @($dotExe, $dotSetup, $portableZip)) {
    '{0}  {1}' -f (Get-FileHash -LiteralPath (Join-Path $publishDir $name) -Algorithm SHA256).Hash.ToLowerInvariant(), $name
}
Write-Utf8 (Join-Path $publishDir 'SHA256SUMS.txt') (($publishSums -join "`n") + "`n")

# Local metadata is prepared for review; this script never creates a tag or release.
$commit = (& git rev-parse HEAD)
if ($LASTEXITCODE -ne 0) { throw "git rev-parse HEAD failed ($LASTEXITCODE)" }
$commit = $commit.Trim()
$porcelain = @(& git status --porcelain)
if ($LASTEXITCODE -ne 0) { throw "git status --porcelain failed ($LASTEXITCODE)" }
$latest = [ordered]@{
    version = $version
    name = "Mouse Insight $version"
    publication_status = 'awaiting-local-validation'
    source_commit = $commit
    source_dirty = [bool]$porcelain.Count
    url = "https://github.com/lingcang728/MouseInsight/releases/tag/v$version"
    portable = "https://github.com/lingcang728/MouseInsight/releases/download/v$version/Mouse.Insight.exe"
    setup = "https://github.com/lingcang728/MouseInsight/releases/download/v$version/Mouse.Insight_${version}_x64-setup.exe"
    portable_zip = "https://github.com/lingcang728/MouseInsight/releases/download/v$version/Mouse.Insight_${version}_portable.zip"
}
Write-Utf8 (Join-Path $metadataDir 'latest.json') (($latest | ConvertTo-Json) + "`n")
$hashEntries = foreach ($relative in @($exeName, $setupName, 'assets/app.ico', 'metadata/latest.json')) {
    $path = Join-Path $releaseDir $relative
    '{0}  {1}' -f (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant(), $relative
}
Write-Utf8 (Join-Path $metadataDir 'SHA256SUMS.txt') (($hashEntries -join "`n") + "`n")

# Remove only superseded, explicitly recognized files; never recursively clean release.
foreach ($file in $ownedRootFiles) {
    if ($file.Name -eq $exeName -or $file.Name -eq $setupName -or $file.Name -match '^config\.json($|\.)') { continue }
    $path = Assert-ReleasePath $file.FullName
    Remove-Item -LiteralPath $path -Force
}
foreach ($file in @(Get-ChildItem -LiteralPath $stageDir -File)) {
    $path = Assert-ReleasePath $file.FullName
    Remove-Item -LiteralPath $path -Force
}
Remove-Item -LiteralPath (Assert-ReleasePath $stageDir) -Recurse -Force

# Keep the user's existing entry points on the portable executable. Shortcuts are
# only repointed when they already exist; -CreateShortcuts opts in to creating them.
$wsh = New-Object -ComObject WScript.Shell
$programs = [Environment]::GetFolderPath('Programs')
$shortcutPaths = @(
    (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Mouse Insight.lnk'),
    (Join-Path $programs 'Mouse Insight\Mouse Insight.lnk'),
    (Join-Path $programs 'Mouse Insight.lnk')
)
foreach ($path in $shortcutPaths) {
    if (-not $CreateShortcuts -and -not (Test-Path -LiteralPath $path -PathType Leaf)) { continue }
    New-Item -ItemType Directory -Path (Split-Path -Parent $path) -Force | Out-Null
    $shortcut = $wsh.CreateShortcut($path)
    $shortcut.TargetPath = $destExe
    $shortcut.WorkingDirectory = $releaseDir
    $shortcut.IconLocation = (Join-Path $assetsDir 'app.ico') + ',0'
    $shortcut.Description = 'Mouse Insight (Portable)'
    $shortcut.Save()
}
Write-Host "Local package ready: $version (no tag or GitHub release created)"
Write-Host "Configuration: $dataDir"
Write-Host "Previous files backed up: $backupDir"
Get-ChildItem -LiteralPath $releaseDir -Force | Select-Object Name, Length
