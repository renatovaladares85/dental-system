[CmdletBinding()]
param(
    [string]$WixExecutable,
    [string]$DotNetExecutable,
    [string]$ToolsRoot,
    [switch]$ForceRefresh
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
. (Join-Path $PSScriptRoot 'wix-tooling.ps1')

Initialize-OdsWixTooling `
    -WixExecutable $WixExecutable `
    -DotNetExecutable $DotNetExecutable `
    -ToolsRoot $ToolsRoot `
    -ForceRefresh:$ForceRefresh `
    -RepositoryRoot $repositoryRoot
