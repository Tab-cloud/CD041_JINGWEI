param(
    [string]$KicadCli = 'C:\Users\Administrator\AppData\Local\Programs\KiCad\10.0\bin\kicad-cli.exe',
    [string]$ArtifactDirectory = 'testdata\logical\golden-rs485\kicad',
    [string]$ReportPath = 'testdata\logical\golden-rs485\reports\kicad-10.0.5-schematic-gate.json'
)

$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
$lockPath = Join-Path $repository 'toolchains\kicad-10.0.5-windows.json'
$lock = Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json
$cliPath = (Resolve-Path -LiteralPath $KicadCli).Path
$artifactPath = (Resolve-Path -LiteralPath (Join-Path $repository $ArtifactDirectory)).Path
$schematicPath = Join-Path $artifactPath 'design.kicad_sch'
$projectionPath = Join-Path $artifactPath 'design.projection.json'
$ercPath = Join-Path $artifactPath 'design.erc.json'
$renderDirectory = Join-Path $artifactPath 'rendered'
$resolvedReport = Join-Path $repository $ReportPath
$reportDirectory = Split-Path -Parent $resolvedReport
New-Item -ItemType Directory -Force -Path $renderDirectory, $reportDirectory | Out-Null

$versionOutput = (& $cliPath --version 2>&1 | ForEach-Object { "$_" }) -join "`n"
$versionExit = $LASTEXITCODE
$binaryHash = (Get-FileHash -LiteralPath $cliPath -Algorithm SHA256).Hash.ToLowerInvariant()
$versionMatches = $versionExit -eq 0 -and $versionOutput.Trim() -eq $lock.version
$hashMatches = $binaryHash -eq $lock.sha256

$ercOutput = (& $cliPath sch erc --format json --severity-all --exit-code-violations --output $ercPath $schematicPath 2>&1 | ForEach-Object { "$_" }) -join "`n"
$ercExit = $LASTEXITCODE
$svgOutput = (& $cliPath sch export svg --output $renderDirectory $schematicPath 2>&1 | ForEach-Object { "$_" }) -join "`n"
$svgExit = $LASTEXITCODE
$erc = if (Test-Path -LiteralPath $ercPath) { Get-Content -LiteralPath $ercPath -Raw | ConvertFrom-Json } else { $null }
$svgFiles = @(Get-ChildItem -LiteralPath $renderDirectory -Filter '*.svg' -File -ErrorAction SilentlyContinue)
$violationCount = if ($null -ne $erc) { @($erc.sheets | ForEach-Object { $_.violations }).Count } else { -1 }
$zeroViolations = $violationCount -eq 0
$passed = $versionMatches -and $hashMatches -and $ercExit -eq 0 -and $zeroViolations -and $svgExit -eq 0 -and $svgFiles.Count -gt 0

$report = [ordered]@{
    schemaVersion = 'jingwei-kicad-schematic-gate/0.1'
    generatedAt = (Get-Date).ToUniversalTime().ToString('o')
    passed = $passed
    tool = [ordered]@{
        name = 'kicad-cli'
        version = $versionOutput.Trim()
        expectedVersion = $lock.version
        sha256 = $binaryHash
        expectedSha256 = $lock.sha256
        versionMatches = $versionMatches
        hashMatches = $hashMatches
    }
    input = [ordered]@{
        schematic = 'design.kicad_sch'
        schematicSha256 = (Get-FileHash -LiteralPath $schematicPath -Algorithm SHA256).Hash.ToLowerInvariant()
        projection = 'design.projection.json'
        projectionSha256 = (Get-FileHash -LiteralPath $projectionPath -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    erc = [ordered]@{
        exitCode = $ercExit
        zeroViolations = $zeroViolations
        violationCount = $violationCount
        output = $ercOutput
        report = 'design.erc.json'
    }
    render = [ordered]@{
        exitCode = $svgExit
        svgCount = $svgFiles.Count
        output = $svgOutput
        files = @($svgFiles | ForEach-Object { $_.Name })
    }
    commands = @(
        'kicad-cli sch erc --format json --severity-all --exit-code-violations --output <report> <schematic>',
        'kicad-cli sch export svg --output <directory> <schematic>'
    )
}
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $resolvedReport -Encoding utf8

if (-not $passed) {
    throw "KiCad schematic gate failed; inspect $resolvedReport"
}
Write-Output $resolvedReport
