[CmdletBinding()]
param(
    [string]$RepositoryRoot = '',
    [string]$Version = '0.1.0'
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Invalid release version: $Version"
}
$repository = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$gitDirectory = Join-Path $repository '.git'
if (-not (Test-Path -LiteralPath $gitDirectory -PathType Container)) {
    throw "Packaging root is not the independent Jingwei Git repository: $repository"
}

Push-Location $repository
try {
    $dirty = @(git status --porcelain)
    if ($LASTEXITCODE -ne 0 -or $dirty.Count -ne 0) {
        throw 'Release packaging requires a clean committed worktree'
    }

    $distRoot = Join-Path $repository 'dist'
    New-Item -ItemType Directory -Path $distRoot -Force | Out-Null
    $resolvedDistRoot = (Resolve-Path -LiteralPath $distRoot).Path
    $versionDist = Join-Path $resolvedDistRoot "v$Version"
    New-Item -ItemType Directory -Path $versionDist -Force | Out-Null
    $resolvedVersionDist = (Resolve-Path -LiteralPath $versionDist).Path
    if ((Split-Path -Parent $resolvedVersionDist) -ne $resolvedDistRoot) {
        throw 'Versioned package directory escaped dist'
    }

    # Remove legacy top-level package outputs and stale files for this exact
    # version. This leaves dist/v<version> with one user-facing EXE only.
    foreach ($file in @(Get-ChildItem -LiteralPath $resolvedDistRoot -File)) {
        if ((Resolve-Path -LiteralPath $file.DirectoryName).Path -ne $resolvedDistRoot) {
            throw 'Top-level package cleanup escaped dist'
        }
        Remove-Item -LiteralPath $file.FullName -Force
    }
    foreach ($file in @(Get-ChildItem -LiteralPath $resolvedVersionDist -File)) {
        if ((Resolve-Path -LiteralPath $file.DirectoryName).Path -ne $resolvedVersionDist) {
            throw 'Version package cleanup escaped its owned directory'
        }
        Remove-Item -LiteralPath $file.FullName -Force
    }
    if (@(Get-ChildItem -LiteralPath $resolvedVersionDist -Directory -Force).Count -ne 0) {
        throw 'Versioned package directory contains unexpected subdirectories'
    }

    & (Join-Path $repository 'scripts\verify-windows-installer-hooks.ps1') `
        -RepositoryRoot $repository -ExpectedVersion $Version
    if ($LASTEXITCODE -ne 0) { throw 'Windows installer lifecycle hook verification failed' }
    & (Join-Path $repository 'scripts\test-windows-installer-lock-cleanup.ps1') `
        -RepositoryRoot $repository
    if ($LASTEXITCODE -ne 0) { throw 'Windows installer lifecycle live test failed' }

    $evidenceDirectory = Join-Path $repository "target\package-evidence\v$Version"
    New-Item -ItemType Directory -Path $evidenceDirectory -Force | Out-Null
    $sidecarGatePath = Join-Path $evidenceDirectory 'windows-sidecar-gate.json'
    & (Join-Path $repository 'scripts\verify-windows-sidecars.ps1') `
        -RepositoryRoot $repository -ReportPath $sidecarGatePath
    if ($LASTEXITCODE -ne 0) { throw 'Pinned sidecar verification failed' }

    Push-Location (Join-Path $repository 'app')
    try {
        pnpm tauri build
        if ($LASTEXITCODE -ne 0) { throw "Tauri NSIS build failed: $LASTEXITCODE" }
    }
    finally {
        Pop-Location
    }

    $bundleDirectory = Join-Path $repository 'app\src-tauri\target\release\bundle\nsis'
    $installers = @(Get-ChildItem -LiteralPath $bundleDirectory -Filter '*.exe' -File |
        Where-Object { $_.Name -like "Jingwei Agent_${Version}_*_x64-setup.exe" -or $_.Name -eq "Jingwei Agent_${Version}_x64-setup.exe" })
    if ($installers.Count -eq 0) {
        $installers = @(Get-ChildItem -LiteralPath $bundleDirectory -Filter "*${Version}*x64-setup.exe" -File)
    }
    if ($installers.Count -ne 1) {
        throw "Expected exactly one NSIS installer for version $Version, found $($installers.Count)"
    }

    $distInstaller = Join-Path $resolvedVersionDist "Jingwei-Agent_${Version}_x64-setup.exe"
    Copy-Item -LiteralPath $installers[0].FullName -Destination $distInstaller -Force
    $versionInfo = (Get-Item -LiteralPath $distInstaller).VersionInfo
    if ($versionInfo.FileVersion -ne $Version -or $versionInfo.ProductVersion -ne $Version) {
        throw "Installer version metadata does not match $Version"
    }
    $outputs = @(Get-ChildItem -LiteralPath $resolvedVersionDist -Force)
    if ($outputs.Count -ne 1 -or $outputs[0].FullName -ne $distInstaller -or $outputs[0].Extension -ne '.exe') {
        throw 'Versioned dist must contain exactly one installer EXE'
    }

    Write-Output $distInstaller
}
finally {
    Pop-Location
}
