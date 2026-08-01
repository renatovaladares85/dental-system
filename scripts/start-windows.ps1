[CmdletBinding()]
param(
    [switch]$SkipTests,
    [switch]$OpenBrowser
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$developmentScripts = Join-Path $PSScriptRoot 'dev'

& (Join-Path $developmentScripts 'check.ps1')
& (Join-Path $developmentScripts 'build.ps1') -SkipChecks -SkipTests:$SkipTests
& (Join-Path $developmentScripts 'run.ps1') -OpenBrowser:$OpenBrowser
