[CmdletBinding()]
param(
    [string]$RepositoryRoot = '',
    [string]$Version = '0.1.0'
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}
$repository = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$gitDirectory = Join-Path $repository '.git'
if (-not (Test-Path -LiteralPath $gitDirectory)) {
    throw "Packaging root is not the independent Jingwei Git repository: $repository"
}

Push-Location $repository
try {
    $dirty = @(git status --porcelain)
    if ($LASTEXITCODE -ne 0 -or $dirty.Count -ne 0) {
        throw 'Release provenance requires a clean committed worktree before packaging'
    }
    $commit = (git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Unable to resolve release commit' }

    $dist = Join-Path $repository 'dist'
    New-Item -ItemType Directory -Path $dist -Force | Out-Null
    $sidecarGatePath = Join-Path $dist 'windows-sidecar-gate.json'
    & (Join-Path $repository 'scripts\verify-windows-sidecars.ps1') -RepositoryRoot $repository -ReportPath $sidecarGatePath
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
    $installer = $installers[0]
    $distInstaller = Join-Path $dist "Jingwei-Agent_${Version}_x64-setup.exe"
    Copy-Item -LiteralPath $installer.FullName -Destination $distInstaller -Force

    $rootMetadata = cargo metadata --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) { throw 'Root Cargo metadata collection failed' }
    $tauriMetadata = cargo metadata --format-version 1 --locked --manifest-path (Join-Path $repository 'app\src-tauri\Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'Tauri Cargo metadata collection failed' }
    $pnpmLicenses = pnpm licenses list --json --prod
    if ($LASTEXITCODE -ne 0) { throw 'pnpm license inventory failed' }

    $components = [Collections.Generic.List[object]]::new()
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($metadataText in @($rootMetadata, $tauriMetadata)) {
        $metadata = $metadataText | ConvertFrom-Json
        foreach ($package in $metadata.packages) {
            $key = "cargo:$($package.name)@$($package.version)"
            if ($seen.Add($key)) {
                $components.Add([ordered]@{
                    type = 'library'
                    name = $package.name
                    version = $package.version
                    purl = "pkg:cargo/$($package.name)@$($package.version)"
                    licenses = @([ordered]@{ license = [ordered]@{ name = if ($package.license) { $package.license } else { 'NOASSERTION' } } })
                })
            }
        }
    }
    $licenseGroups = $pnpmLicenses | ConvertFrom-Json
    foreach ($licenseProperty in $licenseGroups.PSObject.Properties) {
        foreach ($package in $licenseProperty.Value) {
            foreach ($packageVersion in $package.versions) {
                $key = "npm:$($package.name)@$packageVersion"
                if ($seen.Add($key)) {
                    $components.Add([ordered]@{
                        type = 'library'
                        name = $package.name
                        version = $packageVersion
                        purl = "pkg:npm/$([Uri]::EscapeDataString($package.name))@$packageVersion"
                        licenses = @([ordered]@{ license = [ordered]@{ name = $licenseProperty.Name } })
                    })
                }
            }
        }
    }
    $orderedComponents = @($components | Sort-Object name, version, purl)
    $timestamp = [DateTime]::UtcNow.ToString('o')
    $sbom = [ordered]@{
        bomFormat = 'CycloneDX'
        specVersion = '1.5'
        serialNumber = "urn:uuid:$([Guid]::NewGuid())"
        version = 1
        metadata = [ordered]@{
            timestamp = $timestamp
            component = [ordered]@{ type = 'application'; name = 'Jingwei Agent'; version = $Version }
        }
        components = $orderedComponents
    }
    $sbomPath = Join-Path $dist 'jingwei-agent.cdx.json'
    $sbom | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $sbomPath -Encoding utf8

    $noticeLines = [Collections.Generic.List[string]]::new()
    $noticeLines.Add('# Jingwei Agent third-party dependency notice')
    $noticeLines.Add('')
    $noticeLines.Add("Generated from locked Cargo and pnpm dependency metadata for $commit.")
    $noticeLines.Add('This inventory identifies declared licenses; the corresponding upstream license texts remain authoritative.')
    $noticeLines.Add('')
    foreach ($component in $orderedComponents) {
        $licenseId = $component.licenses[0].license.name
        $noticeLines.Add("- $($component.name) $($component.version) — $licenseId")
    }
    $noticePath = Join-Path $dist 'THIRD_PARTY_NOTICES.md'
    $noticeLines | Set-Content -LiteralPath $noticePath -Encoding utf8

    $signature = Get-AuthenticodeSignature -LiteralPath $distInstaller
    $installerHash = (Get-FileHash -LiteralPath $distInstaller -Algorithm SHA256).Hash.ToLowerInvariant()
    $sidecarManifestPath = Join-Path $repository 'app\src-tauri\sidecars\manifest.json'
    $materials = @(
        'Cargo.lock',
        'pnpm-lock.yaml',
        'app/src-tauri/Cargo.lock',
        'app/src-tauri/sidecars/manifest.json'
    ) | ForEach-Object {
        $path = Join-Path $repository ($_ -replace '/', '\')
        [ordered]@{
            path = $_
            sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    $provenance = [ordered]@{
        schemaVersion = 'jingwei-build-provenance/0.1'
        subject = @([ordered]@{
            name = (Split-Path -Leaf $distInstaller)
            sha256 = $installerHash
            sizeBytes = (Get-Item -LiteralPath $distInstaller).Length
        })
        build = [ordered]@{
            gitCommit = $commit
            gitWorktreeClean = $true
            target = 'x86_64-pc-windows-msvc'
            generatedAt = $timestamp
            rustc = (rustc --version)
            cargo = (cargo --version)
            node = (node --version)
            pnpm = (pnpm --version)
            bun = (bun --version)
        }
        materials = $materials
        sidecarManifestSha256 = (Get-FileHash -LiteralPath $sidecarManifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
        authenticode = [ordered]@{
            status = $signature.Status.ToString()
            signer = if ($signature.SignerCertificate) { $signature.SignerCertificate.Subject } else { $null }
        }
        productionRelease = $false
        openGates = @(
            'Authenticode organization signing certificate is not configured',
            'signed updater and rollback channel are not configured',
            'manufacturer DFM and physical prototype evidence are not available',
            'organization Production Release signature is not available'
        )
    }
    $provenancePath = Join-Path $dist 'build-provenance.json'
    $provenance | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $provenancePath -Encoding utf8

    $gate = [ordered]@{
        schemaVersion = 'jingwei-windows-alpha-package-gate/0.1'
        installer = [ordered]@{
            path = (Split-Path -Leaf $distInstaller)
            sha256 = $installerHash
            sizeBytes = (Get-Item -LiteralPath $distInstaller).Length
            authenticode = $signature.Status.ToString()
        }
        sbom = [ordered]@{
            path = (Split-Path -Leaf $sbomPath)
            componentCount = $orderedComponents.Count
            sha256 = (Get-FileHash -LiteralPath $sbomPath -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        thirdPartyNotices = [ordered]@{
            path = (Split-Path -Leaf $noticePath)
            sha256 = (Get-FileHash -LiteralPath $noticePath -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        provenance = [ordered]@{
            path = (Split-Path -Leaf $provenancePath)
            gitCommit = $commit
        }
        sidecarGate = 'passed'
        commercialOpenAiLiveTest = [ordered]@{
            status = 'skipped'
            reason = 'OPENAI_API_KEY and JINGWEI_OPENAI_MODEL are not set'
        }
        productionRelease = $false
    }
    $gatePath = Join-Path $dist 'windows-alpha-package-gate.json'
    $gate | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $gatePath -Encoding utf8
    Write-Output $distInstaller
}
finally {
    Pop-Location
}
