[CmdletBinding()]
param(
    [switch]$SkipChecks,
    [switch]$SkipTests,
    [switch]$Clean
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-OdsPowerShell7
Write-Host '[1/12] Validando ambiente'
if (-not $SkipChecks) {
    & (Join-Path $PSScriptRoot 'check.ps1')
} else {
    Update-OdsProcessPath
    Import-OdsVisualStudioEnvironment
}

if ($Clean) {
    foreach ($path in @(
        (Join-Path $script:RepositoryRoot 'dist'),
        (Join-Path $script:RepositoryRoot 'src-tauri\target')
    )) {
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force }
    }
}

Push-Location $script:RepositoryRoot
try {
    Write-Host '[2/12] Instalando dependências frontend'
    $null = Invoke-OdsNpm 'ci' '--include=dev'
    Write-Host '[3/12] Verificando formatação'
    $null = Invoke-OdsNpm 'run' 'format:check'
    Write-Host '[4/12] Executando lint'
    $null = Invoke-OdsNpm 'run' 'lint'
    Write-Host '[5/12] Executando typecheck'
    $null = Invoke-OdsNpm 'run' 'typecheck'
    Write-Host '[6/12] Executando testes frontend'
    if ($SkipTests) { Write-Host 'Testes frontend ignorados por -SkipTests.' } else { $null = Invoke-OdsNpm 'test' }
    Write-Host '[7/12] Compilando frontend'
    $null = Invoke-OdsNpm 'run' 'build'
    Write-Host '[8/12] Verificando PWA'
    $null = Invoke-OdsNpm 'run' 'verify:pwa'

    Write-Host '[9/12] Verificando Rustfmt'
    $null = Invoke-OdsNative 'cargo.exe' 'fmt' '--manifest-path' $script:CargoManifest '--all' '--' '--check'
    Write-Host '[10/12] Executando Clippy'
    $null = Invoke-OdsNative 'cargo.exe' 'clippy' '--manifest-path' $script:CargoManifest '--locked' '--all-targets' '--all-features' '--' '-D' 'warnings'
    Write-Host '[11/12] Executando testes Rust'
    if ($SkipTests) { Write-Host 'Testes Rust ignorados por -SkipTests.' } else { $null = Invoke-OdsNative 'cargo.exe' 'test' '--manifest-path' $script:CargoManifest '--locked' '--all-features' }
    Write-Host '[12/12] Compilando backend'
    $null = Invoke-OdsNative 'cargo.exe' 'build' '--manifest-path' $script:CargoManifest '--locked' '--all-features'

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
