[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$manifest = Join-Path $repositoryRoot 'src-tauri\Cargo.toml'
$toolchainFile = Join-Path $repositoryRoot 'rust-toolchain.toml'

foreach ($requiredFile in @($manifest, $toolchainFile)) {
    if (-not (Test-Path -LiteralPath $requiredFile -PathType Leaf)) {
        throw "Arquivo obrigatório ausente: '$requiredFile'."
    }
}
if (-not (Get-Command 'cargo.exe' -ErrorAction SilentlyContinue)) {
    throw 'cargo.exe não está disponível; a toolchain Rust fixada deve ser instalada antes desta suíte.'
}
$match = Select-String -Path $toolchainFile -Pattern '^channel = "([^"]+)"$'
if (-not $match) { throw 'Canal Rust ausente em rust-toolchain.toml.' }
$channel = $match.Matches[0].Groups[1].Value
$global:LASTEXITCODE = 0

Push-Location $repositoryRoot
try {
    & cargo.exe "+$channel" test --manifest-path $manifest --locked windows_service
    if ($LASTEXITCODE -ne 0) {
        throw "A suíte de lifecycle do serviço Windows falhou com código $LASTEXITCODE."
    }
} finally {
    Pop-Location
}
