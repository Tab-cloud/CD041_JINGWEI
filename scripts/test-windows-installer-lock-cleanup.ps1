[CmdletBinding()]
param([string]$RepositoryRoot = '')

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}
$repository = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$targetRoot = Join-Path $repository 'target'
New-Item -ItemType Directory -Path $targetRoot -Force | Out-Null
$resolvedTargetRoot = (Resolve-Path -LiteralPath $targetRoot).Path
$testRoot = Join-Path $resolvedTargetRoot ("installer-hook-test-" + [Guid]::NewGuid().ToString('N'))
$installRoot = Join-Path $testRoot 'installed'
$sidecarRoot = Join-Path $installRoot 'sidecars\bin'
$runtimeName = 'jw-hook-test.exe'
$runtimeFixture = Join-Path $sidecarRoot $runtimeName
$marker = Join-Path $testRoot 'installer-continued.txt'
$legacyMarker = Join-Path $testRoot 'legacy-uninstaller-ran.txt'
$harnessSource = Join-Path $testRoot 'installer-hook-test.nsi'
$harnessExe = Join-Path $testRoot 'installer-hook-test.exe'
$legacyUninstallerSource = Join-Path $testRoot 'legacy-uninstaller-test.nsi'
$legacyUninstallerExe = Join-Path $testRoot 'legacy-uninstaller-test.exe'
$legacyRegistryKey = "Software\JingweiAgent\InstallerHookTest\$([Guid]::NewGuid().ToString('N'))"
$legacyRegistryPath = "Registry::HKEY_CURRENT_USER\$legacyRegistryKey"
$hookPath = (Resolve-Path -LiteralPath (Join-Path $repository 'app\src-tauri\windows\installer-hooks.nsh')).Path
$makeNsis = Join-Path $env:LOCALAPPDATA 'tauri\NSIS\makensis.exe'
$fixtureProcess = $null

try {
    New-Item -ItemType Directory -Path $sidecarRoot -Force | Out-Null
    Copy-Item -LiteralPath $env:ComSpec -Destination $runtimeFixture
    if (-not (Test-Path -LiteralPath $makeNsis -PathType Leaf)) {
        throw "Tauri NSIS compiler is unavailable: $makeNsis"
    }

    $legacySource = @"
Unicode true
Name "Jingwei legacy uninstaller test"
OutFile "$legacyUninstallerExe"
SilentInstall silent
RequestExecutionLevel user
Section
  DeleteRegKey HKCU "$legacyRegistryKey"
  FileOpen `$0 "$legacyMarker" w
  FileWrite `$0 "removed"
  FileClose `$0
SectionEnd
"@
    Set-Content -LiteralPath $legacyUninstallerSource -Value $legacySource -Encoding utf8
    & $makeNsis '/V2' $legacyUninstallerSource
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $legacyUninstallerExe -PathType Leaf)) {
        throw 'Unable to compile isolated legacy uninstaller fixture'
    }
    New-Item -Path $legacyRegistryPath -Force | Out-Null
    New-ItemProperty -Path $legacyRegistryPath -Name 'DisplayName' -Value 'Jingwei Agent Dev' -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $legacyRegistryPath -Name 'UninstallString' -Value ('"' + $legacyUninstallerExe + '"') -PropertyType String -Force | Out-Null

    $source = @"
Unicode true
Name "Jingwei installer hook live test"
OutFile "$harnessExe"
SilentInstall silent
RequestExecutionLevel user
InstallDir "$installRoot"
!define JINGWEI_WORKBENCH_IMAGE "jw-hook-ui.exe"
!define JINGWEI_RUNTIME_IMAGE "$runtimeName"
!define JINGWEI_DAEMON_IMAGE "jw-hook-daemon.exe"
!define JINGWEI_LEGACY_UNINSTALL_KEY "$legacyRegistryKey"
!include "$hookPath"
Section
  !insertmacro NSIS_HOOK_PREINSTALL
  FileOpen `$0 "$marker" w
  FileWrite `$0 "continued"
  FileClose `$0
SectionEnd
"@
    Set-Content -LiteralPath $harnessSource -Value $source -Encoding utf8
    & $makeNsis '/V2' $harnessSource
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $harnessExe -PathType Leaf)) {
        throw 'Unable to compile isolated NSIS lifecycle harness'
    }

    $fixtureProcess = Start-Process -FilePath $runtimeFixture `
        -ArgumentList @('/d', '/c', 'ping.exe -n 120 127.0.0.1 >NUL') `
        -WindowStyle Hidden -PassThru
    Start-Sleep -Milliseconds 300
    if ($fixtureProcess.HasExited) {
        throw 'Lock fixture exited before the installer hook test began'
    }

    $harness = Start-Process -FilePath $harnessExe -ArgumentList '/S' -WindowStyle Hidden -PassThru -Wait
    $fixtureProcess.Refresh()
    if ($harness.ExitCode -ne 0) {
        throw "Installer lifecycle harness failed with exit code $($harness.ExitCode)"
    }
    if (-not $fixtureProcess.HasExited) {
        throw 'Installer lifecycle hook did not stop the locked fixture process'
    }
    if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) {
        throw 'Installer lifecycle hook did not continue after releasing the locked fixture'
    }
    if (-not (Test-Path -LiteralPath $legacyMarker -PathType Leaf)) {
        throw 'Installer lifecycle hook did not invoke the legacy Dev uninstaller'
    }
    if (Test-Path -LiteralPath $legacyRegistryPath) {
        throw 'Installer lifecycle hook continued while the legacy Dev registration remained'
    }

    Write-Output 'Windows installer locked-file cleanup and legacy replacement live test passed'
}
finally {
    if ($null -ne $fixtureProcess) {
        $fixtureProcess.Refresh()
        if (-not $fixtureProcess.HasExited) {
            Stop-Process -Id $fixtureProcess.Id -Force -ErrorAction SilentlyContinue
        }
    }
    if (Test-Path -LiteralPath $testRoot) {
        $resolvedTestRoot = (Resolve-Path -LiteralPath $testRoot).Path
        if ((Split-Path -Parent $resolvedTestRoot) -ne $resolvedTargetRoot -or
            -not (Split-Path -Leaf $resolvedTestRoot).StartsWith('installer-hook-test-', [StringComparison]::Ordinal)) {
            throw 'Refusing to clean an unexpected installer hook test directory'
        }
        Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
    }
    if (Test-Path -LiteralPath $legacyRegistryPath) {
        if ($legacyRegistryKey -notmatch '^Software\\JingweiAgent\\InstallerHookTest\\[0-9a-f]{32}$') {
            throw 'Refusing to clean an unexpected installer hook test registry key'
        }
        Remove-Item -LiteralPath $legacyRegistryPath -Recurse -Force
    }
}
