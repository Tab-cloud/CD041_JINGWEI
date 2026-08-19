[CmdletBinding()]
param(
    [string]$RepositoryRoot = ''
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}
$repository = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$target = 'x86_64-pc-windows-msvc'
$sidecarDirectory = Join-Path $repository 'app\src-tauri\sidecars\bin'
$daemonSource = Join-Path $repository 'target\release\jingwei-daemon.exe'
$daemonTarget = Join-Path $sidecarDirectory "jingwei-daemon-$target.exe"
$runtimeTarget = Join-Path $sidecarDirectory "jingwei-agent-runtime-$target.exe"
$manifestPath = Join-Path $repository 'app\src-tauri\sidecars\manifest.json'
New-Item -ItemType Directory -Path $sidecarDirectory -Force | Out-Null

Push-Location $repository
try {
    cargo build --release -p jingwei-daemon
    if ($LASTEXITCODE -ne 0) { throw "Rust daemon release build failed: $LASTEXITCODE" }
    bun build packages/agent-runtime-sidecar/src/main.ts --compile --outfile $runtimeTarget
    if ($LASTEXITCODE -ne 0) { throw "Runtime sidecar compile failed: $LASTEXITCODE" }
}
finally {
    Pop-Location
}

Copy-Item -LiteralPath $daemonSource -Destination $daemonTarget -Force
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
foreach ($entry in $manifest.sidecars) {
    $binary = if ($entry.kind -eq 'daemon') { $daemonTarget } else { $runtimeTarget }
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        throw "Sidecar binary missing after build: $binary"
    }
    $entry.sha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
    $entry.bundled = $true
}
$manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $manifestPath -Encoding utf8
Write-Output "Materialized pinned Windows sidecars in $sidecarDirectory"
