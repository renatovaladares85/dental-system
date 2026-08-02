[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot '..\dev\common.ps1')

Assert-OdsPowerShell7
Update-OdsProcessPath

$npm = Get-OdsNpmCommand
foreach ($path in $npm.NodePath, $npm.NpmCommandPath, $npm.NpmCliPath) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Executável ou script npm não encontrado: '$path'."
    }
}

$directResult = Invoke-OdsNative $npm.NodePath $npm.NpmCliPath '--version'
$invokeResult = Invoke-OdsNpm '--version'
$directVersion = $directResult.StdOut.Trim()
$invokeVersion = $invokeResult.StdOut.Trim()
if ($directVersion -cne $invokeVersion) {
    throw "Invoke-OdsNpm usa versão '$invokeVersion', diferente da execução direta '$directVersion'."
}

Write-Host "Node: $($npm.NodePath)"
Write-Host "npm.cmd: $($npm.NpmCommandPath)"
Write-Host "npm-cli.js: $($npm.NpmCliPath)"
Write-Host "SUCESSO: Invoke-OdsNpm usa npm $invokeVersion vinculado ao Node selecionado." -ForegroundColor Green
