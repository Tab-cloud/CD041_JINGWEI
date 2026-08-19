param(
    [string]$RepositoryRoot = ''
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}
$repository = (Resolve-Path -LiteralPath $RepositoryRoot).Path

$requiredFiles = @(
    'README.md',
    'docs\01-system-architecture.md',
    'docs\02-domain-model-and-artifacts.md',
    'docs\07-roadmap-and-acceptance.md',
    'docs\08-open-questions-and-decisions.md',
    'docs\13-authoritative-boundaries-and-lifecycle.md',
    'docs\14-design-convergence-audit-and-plan.md',
    'docs\handoffs\integration-2026-08-19.md'
)
foreach ($relativePath in $requiredFiles) {
    $path = Join-Path $repository $relativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required design document is missing: $relativePath"
    }
}

$markdownFiles = @(
    Get-Item -LiteralPath (Join-Path $repository 'README.md')
    Get-ChildItem -LiteralPath (Join-Path $repository 'docs') -Recurse -File -Filter '*.md'
    Get-ChildItem -LiteralPath (Join-Path $repository 'app') -File -Filter '*.md'
    Get-ChildItem -LiteralPath (Join-Path $repository 'packages') -Recurse -File -Filter '*.md'
    Get-ChildItem -LiteralPath (Join-Path $repository 'bindings') -Recurse -File -Filter '*.md'
    Get-ChildItem -LiteralPath (Join-Path $repository 'testdata') -Recurse -File -Filter '*.md'
)

$brokenLinks = [System.Collections.Generic.List[string]]::new()
$linkPattern = [regex]'\[[^\]]+\]\((?<target>[^)]+)\)'
foreach ($file in $markdownFiles) {
    $content = Get-Content -LiteralPath $file.FullName -Raw
    foreach ($match in $linkPattern.Matches($content)) {
        $target = $match.Groups['target'].Value.Trim().Trim('<', '>')
        if ($target -match '^(?:https?|mailto):' -or $target.StartsWith('#')) {
            continue
        }
        $pathPart = ($target -split '#', 2)[0]
        if ([string]::IsNullOrWhiteSpace($pathPart)) {
            continue
        }
        $pathPart = [Uri]::UnescapeDataString($pathPart)
        $resolvedTarget = [IO.Path]::GetFullPath((Join-Path $file.DirectoryName $pathPart))
        if (-not (Test-Path -LiteralPath $resolvedTarget)) {
            $relativeFile = [IO.Path]::GetRelativePath($repository, $file.FullName)
            $brokenLinks.Add("$relativeFile -> $target")
        }
    }
}
if ($brokenLinks.Count -gt 0) {
    throw "Broken local Markdown links:`n$($brokenLinks -join "`n")"
}

$rootReadme = Get-Content -LiteralPath (Join-Path $repository 'README.md') -Raw
foreach ($requiredReference in @(
    'docs/13-authoritative-boundaries-and-lifecycle.md',
    'docs/14-design-convergence-audit-and-plan.md'
)) {
    if (-not $rootReadme.Contains($requiredReference)) {
        throw "README does not reference authoritative document: $requiredReference"
    }
}

$integration = Get-Content -LiteralPath (Join-Path $repository 'docs\handoffs\integration-2026-08-19.md') -Raw
if (-not $integration.Contains('Foundation integration accepted / Phase 0 product exit blocked / Production Release blocked')) {
    throw 'Integration handoff still conflates foundation integration with Phase 0 product exit'
}

$domainModel = Get-Content -LiteralPath (Join-Path $repository 'docs\02-domain-model-and-artifacts.md') -Raw
if ($domainModel.Contains('status: "draft" | "candidate" | "verified" | "released"')) {
    throw 'Domain model still conflates revision status with maturity/release state'
}
foreach ($canonicalType in @(
    'interface RevisionSnapshot',
    'interface RevisionCommitRecord',
    'interface ChangeProposal',
    'interface ProposalDecisionRecord'
)) {
    if (-not $domainModel.Contains($canonicalType)) {
        throw "Domain model is missing canonical lifecycle type: $canonicalType"
    }
}

$boundaries = Get-Content -LiteralPath (Join-Path $repository 'docs\13-authoritative-boundaries-and-lifecycle.md') -Raw
foreach ($boundaryTerm in @('ToolExecutionId', 'CommandInvocationId', 'EffectiveReleaseStatus')) {
    if (-not $boundaries.Contains($boundaryTerm)) {
        throw "Authoritative boundary document is missing: $boundaryTerm"
    }
}

$gates = Get-Content -LiteralPath (Join-Path $repository 'docs\05-validation-and-production-gates.md') -Raw
if (-not $gates.Contains('L0-L4') -or -not $gates.Contains('`ReleaseRecord`')) {
    throw 'Maturity gates do not separate engineering assessment from L5 Production Release'
}

$platform = Get-Content -LiteralPath (Join-Path $repository 'docs\10-rust-tauri-platform-architecture.md') -Raw
foreach ($ownerMode in @('desktop-owned', 'attached', 'headless')) {
    if (-not $platform.Contains($ownerMode)) {
        throw "Platform architecture does not define sidecar owner mode: $ownerMode"
    }
}

Write-Output "Design document gate passed: $($markdownFiles.Count) Markdown files checked."
