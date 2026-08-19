[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot

Push-Location $repository
try {
    cargo run -p jingwei-jdp --bin jingwei-codegen -- bindings/typescript
    git diff --exit-code -- bindings/typescript
} finally {
    Pop-Location
}
