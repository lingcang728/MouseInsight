[CmdletBinding()]
param(
    [switch]$InstallLocally = $true
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Mouse Insight 生产发布打包工程" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# 1. 动态确定 Cargo 构建输出目录
$cargoTargetDir = if ($env:CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR
} else {
    Join-Path $root 'src-tauri\target'
}
$targetReleaseDir = Join-Path $cargoTargetDir 'release'
$bundleDir = Join-Path $targetReleaseDir 'bundle'
$releaseDir = Join-Path $root 'release'

Write-Host "[1/6] 路径解析:" -ForegroundColor Green
Write-Host "  源码根目录: $root"
Write-Host "  构建输出目录: $cargoTargetDir"
Write-Host "  发布输出目录: $releaseDir"

# 2. 检查是否有当前进程占用目标二进制
$running = Get-Process | Where-Object { $_.Name -match 'Mouse Insight|mouse_insight' }
if ($running) {
    Write-Host "[2/6] 检测到正在运行的 Mouse Insight 进程，尝试安全关闭以解除二进制锁..." -ForegroundColor Yellow
    $running | ForEach-Object {
        try {
            Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
        } catch {}
    }
    Start-Sleep -Milliseconds 600
} else {
    Write-Host "[2/6] 无运行中的冲突进程。" -ForegroundColor Green
}

# 3. 清理 release 目录并保留 config.json
Write-Host "[3/6] 清理并准备 release 目录..." -ForegroundColor Green
if (-not (Test-Path $releaseDir)) {
    New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
} else {
    Get-ChildItem -Path $releaseDir -Force -ErrorAction SilentlyContinue | Where-Object {
        $_.Name -ne 'config.json'
    } | ForEach-Object {
        Remove-Item $_.FullName -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# 4. 执行前端构建与 Tauri 编译
Write-Host "[4/6] 开始执行构建 (TypeScript 校验 + Vite 构建 + Tauri 打包)..." -ForegroundColor Green
npm run tauri:build
if ($LASTEXITCODE -ne 0) {
    throw "Tauri 打包失败，退出码: $LASTEXITCODE"
}

# 5. 归档发布产物
Write-Host "[5/6] 归档发布产物..." -ForegroundColor Green
$copied = 0
$installerPath = $null

# 便携版单文件
$portableExe = Join-Path $targetReleaseDir 'Mouse Insight.exe'
if (Test-Path $portableExe) {
    $destPortable = Join-Path $releaseDir 'Mouse Insight.exe'
    Copy-Item -Path $portableExe -Destination $destPortable -Force
    Write-Host "  已收集便携版: $destPortable" -ForegroundColor DarkCyan
    $copied++
}

# 安装包 (NSIS)
if (Test-Path $bundleDir) {
    Get-ChildItem -Path $bundleDir -Recurse -Include *.exe, *.msi |
        Where-Object { $_.Name -like 'Mouse Insight*' -and $_.FullName -notmatch '\\deps\\' } |
        ForEach-Object {
            $dest = Join-Path $releaseDir $_.Name
            Copy-Item -Path $_.FullName -Destination $dest -Force
            Write-Host "  已收集安装包: $dest" -ForegroundColor DarkCyan
            $copied++
            if ($_.Name -like '*setup.exe' -or $_.Name -like '*installer.exe') {
                $installerPath = $dest
            }
        }
}

if ($copied -lt 1) {
    throw "未在 $cargoTargetDir 中找到任何打包产物！请检查构建配置。"
}

# 6. 生成 SHA-256 校验和 (使用 .NET 原生实现，兼容所有平台环境)
Write-Host "[6/6] 生成发布产物摘要与校验和..." -ForegroundColor Green
function Get-Sha256Hex($filePath) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $stream = [System.IO.File]::OpenRead($filePath)
    try {
        $hashBytes = $sha.ComputeHash($stream)
        ($hashBytes | ForEach-Object { $_.ToString("x2") }) -join ""
    } finally {
        $stream.Close()
        $sha.Dispose()
    }
}

$hashFile = Join-Path $releaseDir 'SHA256SUMS.txt'
$hashEntries = @()

$artifacts = Get-ChildItem -Path $releaseDir | Where-Object { $_.Name -ne 'SHA256SUMS.txt' -and $_.Name -ne 'config.json' }
foreach ($file in $artifacts) {
    $hash = Get-Sha256Hex $file.FullName
    $hashEntries += "$hash  $($file.Name)"
}
$hashEntries | Out-File -FilePath $hashFile -Encoding utf8

Write-Host "`n打包完成！发布文件清单如下:" -ForegroundColor Cyan
Get-ChildItem $releaseDir | Format-Table Name, @{Label="大小(KB)"; Expression={[math]::Round($_.Length / 1KB, 2)}}, LastWriteTime

# 7. 本地安装与快捷方式指向更新
if ($InstallLocally -and $installerPath) {
    Write-Host "========================================" -ForegroundColor Magenta
    Write-Host "  更新本地安装到最新版本..." -ForegroundColor Magenta
    Write-Host "========================================" -ForegroundColor Magenta
    
    Write-Host "正在安装最新版本: $installerPath ..." -ForegroundColor Yellow
    $installProc = Start-Process -FilePath $installerPath -ArgumentList '/S' -Wait -PassThru
    Start-Sleep -Seconds 3

    $installedExe = "$env:LOCALAPPDATA\Programs\Mouse Insight\Mouse Insight.exe"
    if (-not (Test-Path $installedExe)) {
        $installedExe = "$env:LOCALAPPDATA\Mouse Insight\Mouse Insight.exe"
    }

    if (Test-Path $installedExe) {
        Write-Host "本地已成功安装至: $installedExe" -ForegroundColor Green

        $wsh = New-Object -ComObject WScript.Shell
        
        # 1) 桌面快捷方式
        $desktopLnkPath = [System.IO.Path]::Combine([Environment]::GetFolderPath('Desktop'), 'Mouse Insight.lnk')
        $desktopLnk = $wsh.CreateShortcut($desktopLnkPath)
        $desktopLnk.TargetPath = $installedExe
        $desktopLnk.WorkingDirectory = [System.IO.Path]::GetDirectoryName($installedExe)
        $desktopLnk.Description = "Mouse Insight"
        $desktopLnk.IconLocation = "$installedExe,0"
        $desktopLnk.Save()
        Write-Host "已更新桌面快捷方式: $desktopLnkPath -> $installedExe" -ForegroundColor Green

        # 2) 开始菜单快捷方式 (Windows 搜索栏索引路径)
        $startMenuDir = [System.IO.Path]::Combine([Environment]::GetFolderPath('Programs'), 'Mouse Insight')
        if (-not (Test-Path $startMenuDir)) {
            New-Item -ItemType Directory -Force -Path $startMenuDir | Out-Null
        }
        $startMenuLnkPath = [System.IO.Path]::Combine($startMenuDir, 'Mouse Insight.lnk')
        $startMenuLnk = $wsh.CreateShortcut($startMenuLnkPath)
        $startMenuLnk.TargetPath = $installedExe
        $startMenuLnk.WorkingDirectory = [System.IO.Path]::GetDirectoryName($installedExe)
        $startMenuLnk.Description = "Mouse Insight"
        $startMenuLnk.IconLocation = "$installedExe,0"
        $startMenuLnk.Save()
        Write-Host "已更新开始菜单快捷方式 (Windows 搜索栏可直接检索): $startMenuLnkPath -> $installedExe" -ForegroundColor Green

        # 根目录开始菜单也同步一份，确保 Windows Search 瞬时检索
        $rootProgramsLnk = [System.IO.Path]::Combine([Environment]::GetFolderPath('Programs'), 'Mouse Insight.lnk')
        $rootLnk = $wsh.CreateShortcut($rootProgramsLnk)
        $rootLnk.TargetPath = $installedExe
        $rootLnk.WorkingDirectory = [System.IO.Path]::GetDirectoryName($installedExe)
        $rootLnk.Description = "Mouse Insight"
        $rootLnk.IconLocation = "$installedExe,0"
        $rootLnk.Save()
        Write-Host "已更新主程序组快捷方式: $rootProgramsLnk -> $installedExe" -ForegroundColor Green
    } else {
        Write-Warning "未找到安装目录，跳过快捷方式重定向。"
    }
}
