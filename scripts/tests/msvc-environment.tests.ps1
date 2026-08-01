[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot '..\dev\common.ps1')

Update-OdsProcessPath
Import-OdsVisualStudioEnvironment
if (-not (Test-OdsVisualStudioEnvironment)) { throw 'Ambiente MSVC inicial inválido.' }
$first = @('cl.exe', 'link.exe', 'lib.exe', 'rc.exe' | ForEach-Object { (Get-Command $_ -ErrorAction Stop).Source })
$pathBefore = $env:Path
Import-OdsVisualStudioEnvironment
if (-not (Test-OdsVisualStudioEnvironment)) { throw 'Ambiente MSVC perdeu uma ferramenta após segunda importação.' }
$second = @('cl.exe', 'link.exe', 'lib.exe', 'rc.exe' | ForEach-Object { (Get-Command $_ -ErrorAction Stop).Source })
if ($first -join ';' -ne $second -join ';' -or $pathBefore -ne $env:Path) { throw 'A segunda importação MSVC degradou o ambiente.' }
Write-Host 'SUCESSO: importação MSVC idempotente.' -ForegroundColor Green
