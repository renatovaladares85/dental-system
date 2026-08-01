[CmdletBinding()]
param(
    [switch]$SkipChecks,
    [switch]$SkipTests
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-OdsPowerShell7
if (-not $SkipChecks) { & (Join-Path $PSScriptRoot 'check.ps1') }

foreach ($path in @(
    (Join-Path $script:RepositoryRoot 'dist'),
    (Join-Path $script:RepositoryRoot 'src-tauri\target')
)) {
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force }
}

Push-Location $script:RepositoryRoot
try {
    Invoke-OdsNative 'npm.cmd' 'ci'
    Invoke-OdsNative 'npm.cmd' 'run' 'format:check'
    Invoke-OdsNative 'npm.cmd' 'run' 'lint'
    Invoke-OdsNative 'npm.cmd' 'run' 'typecheck'
    if (-not $SkipTests) { Invoke-OdsNative 'npm.cmd' 'test' }
    Invoke-OdsNative 'npm.cmd' 'run' 'build'
    Invoke-OdsNative 'npm.cmd' 'run' 'verify:pwa'

    Invoke-OdsNative 'cargo.exe' 'fmt' '--manifest-path' $script:CargoManifest '--all' '--' '--check'
    Invoke-OdsNative 'cargo.exe' 'clippy' '--manifest-path' $script:CargoManifest '--locked' '--all-targets' '--all-features' '--' '-D' 'warnings'
    if (-not $SkipTests) { Invoke-OdsNative 'cargo.exe' 'test' '--manifest-path' $script:CargoManifest '--locked' '--all-features' }
    Invoke-OdsNative 'cargo.exe' 'build' '--manifest-path' $script:CargoManifest '--locked' '--all-features'

    $executable = Join-Path $script:RepositoryRoot 'src-tauri\target\debug\offline-dental-system.exe'
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw 'O executável de desenvolvimento não foi gerado.' }
    $diagnostics = Invoke-OdsNative $executable '--security-diagnostics' '--json'
    $null = $diagnostics.StdOut | ConvertFrom-Json
    if (-not (Test-Path -LiteralPath (Join-Path $script:RepositoryRoot 'dist\assets') -PathType Container)) {
        throw 'O build do servidor não contém assets reais do frontend.'
    }
} finally {
    Pop-Location
}

Write-Host 'SUCESSO: build integrado concluído.' -ForegroundColor Green
