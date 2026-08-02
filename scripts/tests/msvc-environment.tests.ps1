[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot '..\dev\common.ps1')

Update-OdsProcessPath
Import-OdsVisualStudioEnvironment
if (-not (Test-OdsVisualStudioEnvironment)) { throw 'Ambiente MSVC inicial inválido.' }
$tools = @('cl.exe', 'link.exe', 'lib.exe', 'rc.exe')
$first = [ordered]@{}
foreach ($tool in $tools) {
    $first[$tool] = (Get-Command $tool -ErrorAction Stop).Source
}
$pathBefore = $env:Path
Import-OdsVisualStudioEnvironment
if (-not (Test-OdsVisualStudioEnvironment)) { throw 'Ambiente MSVC perdeu uma ferramenta após segunda importação.' }
$second = [ordered]@{}
foreach ($tool in $tools) {
    $second[$tool] = (Get-Command $tool -ErrorAction Stop).Source
    if (-not $first[$tool].Equals($second[$tool], [StringComparison]::OrdinalIgnoreCase)) {
        throw "A segunda importação MSVC alterou $tool de '$($first[$tool])' para '$($second[$tool])'."
    }
}
if ($pathBefore -cne $env:Path) {
    $beforeEntries = $pathBefore.Split([IO.Path]::PathSeparator, [StringSplitOptions]::RemoveEmptyEntries)
    $afterEntries = $env:Path.Split([IO.Path]::PathSeparator, [StringSplitOptions]::RemoveEmptyEntries)
    $difference = Compare-Object -ReferenceObject $beforeEntries -DifferenceObject $afterEntries |
        ForEach-Object { "$($_.SideIndicator) $($_.InputObject)" }
    throw "A segunda importação MSVC alterou PATH (antes=$($pathBefore.Length), depois=$($env:Path.Length)): $($difference -join '; ')"
}

$targetArchitecture = $env:VSCMD_ARG_TGT_ARCH
try {
    [Environment]::SetEnvironmentVariable('VSCMD_ARG_TGT_ARCH', $null, 'Process')
    if (Test-OdsVisualStudioEnvironment) { throw 'Ambiente MSVC sem arquitetura target foi aceito.' }
} finally {
    [Environment]::SetEnvironmentVariable('VSCMD_ARG_TGT_ARCH', $targetArchitecture, 'Process')
}

$hostArchitecture = $env:VSCMD_ARG_HOST_ARCH
try {
    [Environment]::SetEnvironmentVariable('VSCMD_ARG_HOST_ARCH', $null, 'Process')
    if (Test-OdsVisualStudioEnvironment) { throw 'Ambiente MSVC sem arquitetura host foi aceito.' }
} finally {
    [Environment]::SetEnvironmentVariable('VSCMD_ARG_HOST_ARCH', $hostArchitecture, 'Process')
}

Write-Host 'SUCESSO: importação MSVC idempotente.' -ForegroundColor Green
