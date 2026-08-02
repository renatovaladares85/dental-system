[CmdletBinding(SupportsShouldProcess)]
param([switch]$Force)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$targets = @(
    '.local-data\dev-host',
    '.local-data\logs',
    'dist',
    'coverage',
    'src-tauri\target',
    'node_modules'
) | ForEach-Object { Join-Path $repositoryRoot $_ }

if (-not $Force) {
    $answer = Read-Host "Remover apenas outputs locais de desenvolvimento conhecidos? Digite LIMPAR para confirmar"
    if ($answer -cne 'LIMPAR') { throw 'Limpeza cancelada; nenhum arquivo foi removido.' }
}

foreach ($target in $targets) {
    if (-not (Test-Path -LiteralPath $target)) { continue }
    $item = Get-Item -LiteralPath $target -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Limpeza recusada para reparse point: '$target'."
    }
    if ($PSCmdlet.ShouldProcess($target, 'Remover output local de desenvolvimento')) {
        Remove-Item -LiteralPath $target -Recurse -Force
    }
}
