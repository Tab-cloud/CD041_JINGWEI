[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$KicadCli,

    [string]$RepositoryRoot = '',

    [string]$BoardPath = '',

    [string]$ReportPath = 'testdata\physical\reports\kicad-10.0.5-reference-gate.json'
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}
$repository = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$executable = (Resolve-Path -LiteralPath $KicadCli).Path
$lockPath = Join-Path $repository 'toolchains\kicad-10.0.5-windows.json'
$lock = Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json
$version = (& $executable --version).Trim()
if ($LASTEXITCODE -ne 0 -or $version -ne $lock.version) {
    throw "KiCad version pin failed: expected $($lock.version), observed $version"
}
$hash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
if ($hash -ne $lock.sha256) {
    throw "KiCad executable hash pin failed: expected $($lock.sha256), observed $hash"
}

$boards = if ([string]::IsNullOrWhiteSpace($BoardPath)) {
    @(Get-ChildItem -LiteralPath (Join-Path $repository 'testdata\physical\candidates') -Filter 'board.kicad_pcb' -File -Recurse)
} else {
    @(Get-Item -LiteralPath (Join-Path $repository $BoardPath))
}
if ($boards.Count -ne 1) {
    throw "Expected exactly one KiCad PCB projection, found $($boards.Count)"
}
$board = $boards[0]
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ("jingwei-kicad-reference-" + [Guid]::NewGuid().ToString('N'))
$fabrication = Join-Path $temporaryRoot 'fabrication'
$drcPath = Join-Path $temporaryRoot 'drc.json'
New-Item -ItemType Directory -Path $fabrication -Force | Out-Null

try {
    & $executable pcb drc --format json --output $drcPath --severity-all --exit-code-violations $board.FullName
    if ($LASTEXITCODE -ne 0) {
        throw "KiCad reference DRC failed with exit code $LASTEXITCODE"
    }
    $drc = Get-Content -LiteralPath $drcPath -Raw | ConvertFrom-Json
    if ($drc.kicad_version -ne $lock.version) {
        throw "DRC report version mismatch: expected $($lock.version), observed $($drc.kicad_version)"
    }
    if (@($drc.violations).Count -ne 0 -or @($drc.unconnected_items).Count -ne 0) {
        throw "KiCad DRC gate is not clean"
    }

    & $executable pcb export gerbers --output $fabrication --layers 'F.Cu,B.Cu,F.Mask,B.Mask,F.SilkS,B.SilkS,Edge.Cuts' --subtract-soldermask $board.FullName
    if ($LASTEXITCODE -ne 0) {
        throw "KiCad Gerber export failed with exit code $LASTEXITCODE"
    }
    & $executable pcb export drill --output $fabrication --format excellon --excellon-units mm --excellon-separate-th $board.FullName
    if ($LASTEXITCODE -ne 0) {
        throw "KiCad drill export failed with exit code $LASTEXITCODE"
    }

    $expectedFiles = @(
        'board-F_Cu.gtl',
        'board-B_Cu.gbl',
        'board-F_Mask.gts',
        'board-B_Mask.gbs',
        'board-F_Silkscreen.gto',
        'board-B_Silkscreen.gbo',
        'board-Edge_Cuts.gm1',
        'board-job.gbrjob',
        'board-PTH.drl',
        'board-NPTH.drl'
    )
    $files = foreach ($name in $expectedFiles) {
        $path = Join-Path $fabrication $name
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "KiCad did not emit required fabrication file: $name"
        }
        $file = Get-Item -LiteralPath $path
        if ($file.Length -le 0) {
            throw "KiCad emitted an empty fabrication file: $name"
        }
        [ordered]@{
            name = $name
            byteLength = $file.Length
            sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }

    $boardRelative = [IO.Path]::GetRelativePath($repository, $board.FullName).Replace('\', '/')
    $evidence = [ordered]@{
        schemaVersion = 'jingwei-kicad-reference-gate-v0.1'
        tool = [ordered]@{
            name = 'kicad-cli'
            version = $version
            sha256 = $hash
            pcbFileFormatVersion = $lock.pcbFileFormatVersion
        }
        board = $boardRelative
        drc = [ordered]@{
            exitCode = 0
            violations = @($drc.violations).Count
            unconnectedItems = @($drc.unconnected_items).Count
        }
        fabrication = [ordered]@{
            gerberExitCode = 0
            drillExitCode = 0
            files = @($files)
        }
        commands = @(
            'kicad-cli pcb drc --format json --severity-all --exit-code-violations <board>',
            'kicad-cli pcb export gerbers --layers F.Cu,B.Cu,F.Mask,B.Mask,F.SilkS,B.SilkS,Edge.Cuts --subtract-soldermask <board>',
            'kicad-cli pcb export drill --format excellon --excellon-units mm --excellon-separate-th <board>'
        )
        productionRelease = $false
        remainingGates = @(
            'manufacturer DFM',
            'physical prototype build and test',
            'named engineering and release signoffs'
        )
    }
    $resolvedReportPath = Join-Path $repository $ReportPath
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $resolvedReportPath) | Out-Null
    $evidence | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $resolvedReportPath -Encoding utf8
    Write-Output "KiCad reference gate passed: $resolvedReportPath"
}
finally {
    $resolvedTemporaryRoot = [IO.Path]::GetFullPath($temporaryRoot)
    $resolvedSystemTemporary = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if ($resolvedTemporaryRoot.StartsWith($resolvedSystemTemporary, [StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolvedTemporaryRoot).StartsWith('jingwei-kicad-reference-', [StringComparison]::Ordinal)) {
        Remove-Item -LiteralPath $resolvedTemporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
