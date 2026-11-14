#Requires -Version 7.0
<#
.SYNOPSIS
    Builds RMathTex and assembles the runnable bundle under output/.

.DESCRIPTION
    Compiles the RMathTex binary with Cargo (Release profile by default), then
    copies the executable and the third-party license list into output/.
    The output directory is recreated on every successful build so it never
    contains stale artifacts.

.PARAMETER Configuration
    Cargo build profile: Release (default) or Debug.

.EXAMPLE
    ./scripts/build-crates.ps1

.EXAMPLE
    ./scripts/build-crates.ps1 -Configuration Debug
#>
[CmdletBinding()]
param(
    [ValidateSet('Debug', 'Release')]
    [string]$Configuration = 'Release'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$targetRoot = Join-Path $repoRoot 'target'
$outputRoot = Join-Path $repoRoot 'output'
$binaryBaseName = 'RMathTex'
$binaryFileName = "$binaryBaseName.exe"
$licenseFileName = 'THIRD_PARTY_LICENSES.txt'
$profileFolder = if ($Configuration -eq 'Release') { 'release' } else { 'debug' }

$cargoArguments = @('build', '--bin', $binaryBaseName)
if ($Configuration -eq 'Release') {
    $cargoArguments += '--release'
}

Write-Host "==> cargo $($cargoArguments -join ' ') (Configuration: $Configuration)"
Push-Location $repoRoot
try {
    & cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}

$builtBinary = Join-Path (Join-Path $targetRoot $profileFolder) $binaryFileName
if (-not (Test-Path -LiteralPath $builtBinary -PathType Leaf)) {
    throw "Build artifact not found: $builtBinary"
}

if (Test-Path -LiteralPath $outputRoot) {
    Remove-Item -LiteralPath $outputRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $outputRoot | Out-Null

Copy-Item -LiteralPath $builtBinary -Destination (Join-Path $outputRoot $binaryFileName) -Force

$licenseSource = Join-Path $repoRoot $licenseFileName
if (Test-Path -LiteralPath $licenseSource -PathType Leaf) {
    Copy-Item -LiteralPath $licenseSource -Destination (Join-Path $outputRoot $licenseFileName) -Force
}
else {
    Write-Warning "$licenseFileName not found at $licenseSource; the output bundle is incomplete."
}

$bundleBinary = Join-Path $outputRoot $binaryFileName
$bundleSizeMb = [math]::Round((Get-Item -LiteralPath $bundleBinary).Length / 1MB, 2)
Write-Host "==> Output: $outputRoot"
Write-Host "==> $binaryFileName ($bundleSizeMb MB)"
