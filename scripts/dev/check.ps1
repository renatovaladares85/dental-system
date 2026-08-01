[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-OdsPowerShell7

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
if ((& node.exe --version).Trim().TrimStart('v') -ne $expectedNode) { throw "Node $expectedNode é obrigatório." }
if (([version]((& npm.cmd --version).Trim())).Major -ne 11) { throw 'npm 11 é obrigatório.' }
& rustc.exe "+$channel" -vV | Out-Null
if ($LASTEXITCODE -ne 0) { throw "Rust $channel MSVC não está disponível." }
if ((& rustup.exe target list --installed) -notcontains 'x86_64-pc-windows-msvc') {
    throw 'O target x86_64-pc-windows-msvc não está instalado.'
}

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { throw 'Visual Studio Build Tools não encontrado.' }
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $vs) { throw 'Visual Studio C++ Build Tools não encontrado.' }
if (-not (Get-ChildItem (Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Include') -Directory -ErrorAction SilentlyContinue | Where-Object { Test-Path (Join-Path $_.FullName 'um\Windows.h') })) {
    throw 'Windows SDK não encontrado.'
}

Assert-OdsLocalPathWithoutReparsePoint $script:DevelopmentRoot
Assert-OdsPortsAvailable
Write-Host 'SUCESSO: ambiente de desenvolvimento validado.' -ForegroundColor Green
