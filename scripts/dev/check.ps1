[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-OdsPowerShell7
Update-OdsProcessPath

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or -not [Environment]::Is64BitOperatingSystem) {
    throw 'Windows 11 x64 é obrigatório.'
}
if ([Environment]::OSVersion.Version.Build -lt 22000) { throw 'Windows 11 é obrigatório.' }

$expectedNode = (Get-Content -Raw (Join-Path $script:RepositoryRoot '.nvmrc')).Trim().TrimStart('v')
$channel = ([regex]::Match((Get-Content -Raw (Join-Path $script:RepositoryRoot 'rust-toolchain.toml')), '(?m)^channel\s*=\s*"([^"]+)"$')).Groups[1].Value
if (-not $channel) { throw 'Canal Rust ausente em rust-toolchain.toml.' }

foreach ($command in 'node.exe', 'npm.cmd', 'rustup.exe', 'cargo.exe', 'perl.exe', 'nasm.exe') {
    if (-not (Test-OdsCommand $command)) { throw "Ferramenta obrigatória ausente: $command." }
}
$nodeResult = Invoke-OdsNative 'node.exe' '--version'
$nodeVersion = [string]$nodeResult.StdOut
if ($nodeVersion.Trim().TrimStart('v') -ne $expectedNode) { throw "Node $expectedNode é obrigatório." }
$npmResult = Invoke-OdsNpm '--version'
$npmVersion = [version]([string]$npmResult.StdOut).Trim()
if ($npmVersion.Major -ne 11) { throw 'npm 11 é obrigatório.' }
Invoke-OdsNative 'rustc.exe' "+$channel" '-vV' | Out-Null
$targetResult = Invoke-OdsNative 'rustup.exe' 'target' 'list' '--installed'
$installedTargets = [string]$targetResult.StdOut
if ($installedTargets -notmatch '(?m)^x86_64-pc-windows-msvc$') {
    throw 'O target x86_64-pc-windows-msvc não está instalado.'
}

$vsDeveloperCommand = Get-OdsVsDeveloperCommand
if (-not $vsDeveloperCommand) { throw 'Visual Studio C++ Build Tools não encontrado.' }
if (-not (Get-ChildItem (Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Include') -Directory -ErrorAction SilentlyContinue | Where-Object { Test-Path (Join-Path $_.FullName 'um\Windows.h') })) {
    throw 'Windows SDK não encontrado.'
}

Assert-OdsLocalPathWithoutReparsePoint $script:DevelopmentRoot
Assert-OdsPortsAvailable
Write-Host 'SUCESSO: ambiente de desenvolvimento validado.' -ForegroundColor Green
