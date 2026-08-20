[CmdletBinding()]
param(
    [string]$RepositoryRoot = '',
    [string]$ExpectedVersion = '0.1.0'
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}
$repository = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$tauriRoot = (Resolve-Path -LiteralPath (Join-Path $repository 'app\src-tauri')).Path
$configurationPath = Join-Path $tauriRoot 'tauri.conf.json'
$configuration = Get-Content -LiteralPath $configurationPath -Raw | ConvertFrom-Json

if ($configuration.version -ne $ExpectedVersion) {
    throw "Tauri version $($configuration.version) does not match requested package version $ExpectedVersion"
}
$hookRelativePath = $configuration.bundle.windows.nsis.installerHooks
if ([string]::IsNullOrWhiteSpace($hookRelativePath)) {
    throw 'NSIS installerHooks is not configured'
}
$hookPath = (Resolve-Path -LiteralPath (Join-Path $tauriRoot $hookRelativePath)).Path
if (-not $hookPath.StartsWith($tauriRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'NSIS installer hook escaped the Tauri source directory'
}
$hook = Get-Content -LiteralPath $hookPath -Raw
$requiredTokens = @(
    '!macro NSIS_HOOK_PREINSTALL',
    '!macro NSIS_HOOK_PREUNINSTALL',
    'JingweiInstalledExecutablesLocked',
    'JingweiStopInstalledProcesses',
    'un.JingweiEnsureInstalledProcessesStopped',
    'JingweiRemoveLegacyDevInstallation',
    'JINGWEI_LEGACY_UNINSTALL_KEY',
    'Jingwei Agent Dev.lnk',
    'jingwei-workbench.exe',
    'jingwei-agent-runtime-x86_64-pc-windows-msvc.exe',
    'jingwei-daemon-x86_64-pc-windows-msvc.exe',
    'FileOpen',
    'IfSilent',
    'Abort'
)
foreach ($token in $requiredTokens) {
    if (-not $hook.Contains($token, [StringComparison]::Ordinal)) {
        throw "NSIS installer hook is missing required lifecycle token: $token"
    }
}
$graceful = $hook.IndexOf('/T /IM "${JINGWEI_WORKBENCH_IMAGE}"', [StringComparison]::Ordinal)
$forced = $hook.IndexOf('/F /T /IM "${JINGWEI_WORKBENCH_IMAGE}"', [StringComparison]::Ordinal)
if ($graceful -lt 0 -or $forced -lt 0 -or $graceful -ge $forced) {
    throw 'NSIS installer hook must request graceful workbench exit before forced cleanup'
}

Write-Output "Windows installer lifecycle hook gate passed: $hookPath"
