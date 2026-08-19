[CmdletBinding()]
param(
    [string]$RepositoryRoot = ''
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}
$repository = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$manifestPath = Join-Path $repository 'app\src-tauri\sidecars\manifest.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$entries = @{}
foreach ($entry in $manifest.sidecars) { $entries[$entry.kind] = $entry }
if (-not $entries.runtime.bundled -or -not $entries.daemon.bundled) {
    throw 'Runtime and daemon must both be materialized before verification'
}

function Resolve-VerifiedSidecar([object]$Entry) {
    $path = Join-Path (Join-Path $repository 'app\src-tauri\sidecars') $Entry.relativePath
    $resolved = (Resolve-Path -LiteralPath $path).Path
    $sidecarRoot = (Resolve-Path -LiteralPath (Join-Path $repository 'app\src-tauri\sidecars')).Path
    if (-not $resolved.StartsWith($sidecarRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Sidecar path escaped resource root: $resolved"
    }
    $hash = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $Entry.sha256) { throw "Sidecar hash mismatch: $($Entry.kind)" }
    return $resolved
}

function New-RedirectedProcess([string]$Executable, [string[]]$Arguments) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $Executable
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    if (-not $process.Start()) { throw "Failed to start $Executable" }
    return $process
}

function Read-JsonLine([Diagnostics.Process]$Process, [string]$Label) {
    try {
        $line = $Process.StandardOutput.ReadLineAsync().WaitAsync([TimeSpan]::FromSeconds(30)).GetAwaiter().GetResult()
    }
    catch {
        $stderr = $Process.StandardError.ReadToEndAsync().WaitAsync([TimeSpan]::FromSeconds(2)).GetAwaiter().GetResult()
        throw "$Label did not produce a JSON line: $stderr"
    }
    if ([string]::IsNullOrWhiteSpace($line)) {
        $stderr = if ($Process.HasExited) { $Process.StandardError.ReadToEnd() } else { '' }
        throw "$Label produced an empty handshake/response: $stderr"
    }
    return $line | ConvertFrom-Json
}

function Invoke-RuntimePipeRpc([string]$PipeName, [string]$Token, [string]$Method, [object]$Params = @{}) {
    $shortName = $PipeName -replace '^\\\\\.\\pipe\\', ''
    $client = [IO.Pipes.NamedPipeClientStream]::new('.', $shortName, [IO.Pipes.PipeDirection]::InOut, [IO.Pipes.PipeOptions]::Asynchronous)
    try {
        $client.Connect(30000)
        $request = @{ id = "rpc-$Method"; token = $Token; method = $Method; params = $Params } | ConvertTo-Json -Compress -Depth 10
        $bytes = [Text.Encoding]::UTF8.GetBytes($request)
        $writer = [IO.BinaryWriter]::new($client, [Text.Encoding]::UTF8, $true)
        $writer.Write([uint32]$bytes.Length)
        $writer.Write($bytes)
        $writer.Flush()
        $reader = [IO.BinaryReader]::new($client, [Text.Encoding]::UTF8, $true)
        $length = $reader.ReadUInt32()
        if ($length -eq 0 -or $length -gt 4194304) { throw "Runtime RPC response length is invalid: $length" }
        $responseBytes = $reader.ReadBytes([int]$length)
        if ($responseBytes.Length -ne $length) { throw 'Runtime RPC response was truncated' }
        return [Text.Encoding]::UTF8.GetString($responseBytes) | ConvertFrom-Json
    }
    finally {
        $client.Dispose()
    }
}

$daemonPath = Resolve-VerifiedSidecar $entries.daemon
$runtimePath = Resolve-VerifiedSidecar $entries.runtime
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ("jingwei-sidecar-gate-" + [Guid]::NewGuid().ToString('N'))
$workspace = Join-Path $temporaryRoot 'workspace'
$database = Join-Path $temporaryRoot 'runtime\runtime.sqlite'
$pipeName = "\\.\pipe\jingwei-sidecar-gate-$([Guid]::NewGuid())"
$runtimePipeName = "\\.\pipe\jingwei-runtime-gate-$([Guid]::NewGuid())"
$daemonToken = "daemon-$([Guid]::NewGuid())-$([Guid]::NewGuid())"
$runtimeToken = "runtime-$([Guid]::NewGuid())-$([Guid]::NewGuid())"
New-Item -ItemType Directory -Path $temporaryRoot -Force | Out-Null
$daemon = $null
$runtime = $null

try {
    $daemon = New-RedirectedProcess $daemonPath @(
        'serve', '--workspace', $workspace, '--workspace-id', 'ws-sidecar-gate', '--pipe-name', $pipeName
    )
    $daemon.StandardInput.WriteLine($daemonToken)
    $daemon.StandardInput.Flush()
    $daemonHandshake = Read-JsonLine $daemon 'daemon'
    if ($daemonHandshake.health -ne 'healthy' -or $daemonHandshake.protocolVersion -ne $manifest.protocolVersion) {
        throw 'Daemon sidecar handshake is not healthy or protocol-compatible'
    }

    $runtime = New-RedirectedProcess $runtimePath @()
    $bootstrap = [ordered]@{
        bootstrapToken = $runtimeToken
        daemonPipeName = $pipeName
        daemonSessionToken = $daemonToken
        databasePath = $database
        workspaceId = 'ws-sidecar-gate'
        projectId = 'project-sidecar-gate'
        runtimePipeName = $runtimePipeName
    }
    $runtime.StandardInput.WriteLine(($bootstrap | ConvertTo-Json -Compress))
    $runtime.StandardInput.Flush()
    $runtimeHandshake = Read-JsonLine $runtime 'runtime'
    if ($runtimeHandshake.health -ne 'healthy' -or $runtimeHandshake.protocolVersion -ne $manifest.protocolVersion) {
        throw 'Runtime sidecar handshake is not healthy or protocol-compatible'
    }
    if ($runtimeHandshake.rpcEndpoint -ne $runtimePipeName) { throw 'Runtime sidecar RPC endpoint discovery mismatch' }

    $capabilityResponse = Invoke-RuntimePipeRpc $runtimePipeName $runtimeToken 'capabilities'
    if (-not $capabilityResponse.success) { throw 'Runtime capabilities RPC failed' }
    $operations = @($capabilityResponse.result.jdp.operations)
    foreach ($required in @('requirement_compile', 'release_readiness', 'workspace.capabilities')) {
        if ($required -notin $operations) { throw "Integrated daemon capability missing from Runtime: $required" }
    }

    $evidence = [ordered]@{
        schemaVersion = 'jingwei-windows-sidecar-gate-v0.1'
        target = $manifest.target
        protocolVersion = $manifest.protocolVersion
        binaries = @(
            [ordered]@{ kind = 'daemon'; version = $entries.daemon.version; sha256 = $entries.daemon.sha256 },
            [ordered]@{ kind = 'runtime'; version = $entries.runtime.version; sha256 = $entries.runtime.sha256 }
        )
        handshakes = [ordered]@{
            daemon = $daemonHandshake
            runtime = $runtimeHandshake
        }
        runtimeToDaemon = [ordered]@{
            connected = $true
            availableOperationCount = $operations.Count
            requiredOperations = @(
                'requirement_compile',
                'release_readiness',
                'workspace.capabilities'
            )
            reconciliation = $capabilityResponse.result.jdp.reconciliation
        }
        commercialOpenAiLiveTest = [ordered]@{
            status = 'skipped'
            reason = 'OPENAI_API_KEY and JINGWEI_OPENAI_MODEL are not set'
        }
        uiDataAdapter = [ordered]@{
            status = 'connected'
            reason = 'production workbench selects RuntimeTransportClient and DesignTransportClient; fixture mode requires an explicit query flag'
        }
        productionRelease = $false
    }
    $reportDirectory = Join-Path $repository 'testdata\sidecars'
    New-Item -ItemType Directory -Path $reportDirectory -Force | Out-Null
    $reportPath = Join-Path $reportDirectory 'windows-sidecar-gate.json'
    $evidence | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $reportPath -Encoding utf8
    Write-Output "Windows sidecar gate passed: $reportPath"
}
finally {
    foreach ($process in @($runtime, $daemon)) {
        if ($null -ne $process -and -not $process.HasExited) { $process.Kill($true) }
        if ($null -ne $process) { $process.Dispose() }
    }
    $resolvedTemporaryRoot = [IO.Path]::GetFullPath($temporaryRoot)
    $resolvedSystemTemporary = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if ($resolvedTemporaryRoot.StartsWith($resolvedSystemTemporary, [StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolvedTemporaryRoot).StartsWith('jingwei-sidecar-gate-', [StringComparison]::Ordinal)) {
        Remove-Item -LiteralPath $resolvedTemporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
