[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$manifest = Join-Path $repositoryRoot 'src-tauri\Cargo.toml'

if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
    throw 'Manifesto Cargo do serviço Windows não encontrado.'
}

Push-Location $repositoryRoot
try {
    & cargo.exe test --manifest-path $manifest --locked windows_service
    if ($LASTEXITCODE -ne 0) {
        throw "A suíte de lifecycle do serviço Windows falhou com código $LASTEXITCODE."
    }
} finally {
    Pop-Location
}
