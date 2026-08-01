[CmdletBinding()]
param(
    [switch]$InstallMissing,
    [switch]$SkipTests,
    [string]$ServerBinaryName = 'offline-dental-system'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$script:RepositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$script:ManifestPath = Join-Path $script:RepositoryRoot 'src-tauri\Cargo.toml'
$script:HealthUrl = 'http://127.0.0.1:8742/api/v1/health'
$script:ApplicationUrl = 'http://127.0.0.1:8742'
$script:ExpectedNode = (Get-Content -Raw (Join-Path $script:RepositoryRoot '.nvmrc')).Trim().TrimStart('v')
$script:ExpectedRust = '1.97.1'
$script:DevelopmentRoot = [IO.Path]::GetFullPath((Join-Path $script:RepositoryRoot '.local-data\web-host'))
$script:DevelopmentData = [IO.Path]::GetFullPath((Join-Path $script:DevelopmentRoot 'Data'))
if ($ServerBinaryName -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$') {
    throw 'ServerBinaryName deve ser somente um nome de arquivo seguro, sem diretórios.'
}
$script:DevelopmentServerExecutable = [IO.Path]::GetFullPath(
    (Join-Path $script:RepositoryRoot "src-tauri\target\debug\$ServerBinaryName.exe")
)

function Write-Step([string]$Message) {
    Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Refresh-ProcessPath {
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $cargoPath = Join-Path $env:USERPROFILE '.cargo\bin'
    $portableRoot = Join-Path $env:LOCALAPPDATA 'Programs\StrawberryPerlPortable-5.42.2.1'
    $portablePerl = Join-Path $portableRoot 'perl\bin'
    $portableTools = Join-Path $portableRoot 'c\bin'
    $segments = @($cargoPath, $machinePath, $userPath)
    if (Test-Path -LiteralPath (Join-Path $portablePerl 'perl.exe') -PathType Leaf) {
        $segments += $portablePerl
    }
    if (Test-Path -LiteralPath (Join-Path $portableTools 'nasm.exe') -PathType Leaf) {
        $segments += $portableTools
    }
    $env:Path = $segments -join ';'
}

function Test-Command([string]$Name) {
    return $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}

function ConvertTo-ProcessArguments([string[]]$Arguments) {
    return @($Arguments | ForEach-Object {
        if ($null -eq $_ -or $_.Length -eq 0) { return '""' }
        if ($_ -notmatch '[\s"]') { return $_ }
        return '"' + $_.Replace('"', '\"') + '"'
    })
}

function Invoke-NativeCapture {
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [Parameter(ValueFromRemainingArguments)][string[]]$Arguments
    )
    $stdout = Join-Path ([IO.Path]::GetTempPath()) ("ods-process-" + [guid]::NewGuid().ToString('N') + '.stdout')
    $stderr = Join-Path ([IO.Path]::GetTempPath()) ("ods-process-" + [guid]::NewGuid().ToString('N') + '.stderr')
    try {
        $parameters = @{
            FilePath = $FilePath
            Wait = $true
            NoNewWindow = $true
            PassThru = $true
            RedirectStandardOutput = $stdout
            RedirectStandardError = $stderr
        }
        $processArguments = @(ConvertTo-ProcessArguments $Arguments)
        if ($processArguments.Count -gt 0) { $parameters.ArgumentList = $processArguments }
        $process = Start-Process @parameters
        $output = if (Test-Path -LiteralPath $stdout) { Get-Content -Raw -LiteralPath $stdout } else { '' }
        $errorOutput = if (Test-Path -LiteralPath $stderr) { Get-Content -Raw -LiteralPath $stderr } else { '' }
        return [pscustomobject]@{
            ExitCode = $process.ExitCode
            Output = [string]$output
            ErrorOutput = [string]$errorOutput
        }
    } finally {
        Remove-Item -LiteralPath $stdout, $stderr -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [Parameter(ValueFromRemainingArguments)][string[]]$Arguments
    )
    $parameters = @{
        FilePath = $FilePath
        Wait = $true
        NoNewWindow = $true
        PassThru = $true
    }
    $processArguments = @(ConvertTo-ProcessArguments $Arguments)
    if ($processArguments.Count -gt 0) { $parameters.ArgumentList = $processArguments }
    $process = Start-Process @parameters
    if ($process.ExitCode -ne 0) {
        throw "O comando '$FilePath' falhou com código $($process.ExitCode)."
    }
}

function Require-Winget {
    if (-not (Test-Command 'winget.exe')) {
        throw 'winget não está disponível. Instale/atualize o App Installer da Microsoft Store e execute novamente.'
    }
}

function Install-WingetPackage {
    param(
        [Parameter(Mandatory)][string]$Id,
        [string]$Version,
        [string]$Override,
        [switch]$Force
    )
    Require-Winget
    $arguments = @(
        'install', '--id', $Id, '--exact', '--source', 'winget',
        '--accept-package-agreements', '--accept-source-agreements', '--disable-interactivity'
    )
    if ($Version) { $arguments += @('--version', $Version) }
    if ($Override) { $arguments += @('--override', $Override) }
    if ($Force) { $arguments += '--force' }
    Invoke-Checked 'winget.exe' @arguments
    Refresh-ProcessPath
}

function Assert-OrInstall {
    param(
        [Parameter(Mandatory)][bool]$Condition,
        [Parameter(Mandatory)][string]$Description,
        [Parameter(Mandatory)][scriptblock]$Installer
    )
    if ($Condition) {
        Write-Host "[OK] $Description" -ForegroundColor Green
        return
    }
    if (-not $InstallMissing) {
        throw "$Description não foi encontrado ou está em versão incompatível. Execute novamente com -InstallMissing."
    }
    Write-Host "[INSTALAR] $Description" -ForegroundColor Yellow
    & $Installer
}

function Get-SemanticVersion([string]$Command, [string[]]$Arguments) {
    if (-not (Test-Command $Command)) { return $null }
    $capture = Invoke-NativeCapture $Command @Arguments
    if ($capture.ExitCode -ne 0 -or -not $capture.Output) { return $null }
    $text = $capture.Output -split '\r?\n' | Select-Object -First 1
    $match = [regex]::Match([string]$text, '(\d+\.\d+\.\d+)')
    if (-not $match.Success) { return $null }
    return [version]$match.Groups[1].Value
}

function Find-VsDevCmd {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { return $null }
    $capture = Invoke-NativeCapture $vswhere '-latest' '-products' '*' '-requires' 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64' '-property' 'installationPath'
    if ($capture.ExitCode -ne 0 -or -not $capture.Output) { return $null }
    $installation = ($capture.Output -split '\r?\n' | Select-Object -First 1).Trim()
    $candidate = Join-Path $installation 'Common7\Tools\VsDevCmd.bat'
    if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
    return $null
}

function Test-WindowsSdk {
    $includeRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Include'
    if (-not (Test-Path -LiteralPath $includeRoot -PathType Container)) { return $false }
    return $null -ne (Get-ChildItem -LiteralPath $includeRoot -Directory -ErrorAction SilentlyContinue |
        Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'um\Windows.h') } |
        Select-Object -First 1)
}

function Import-VsDeveloperEnvironment([string]$VsDevCmd) {
    $batch = Join-Path ([IO.Path]::GetTempPath()) ("ods-vsenv-" + [guid]::NewGuid().ToString('N') + '.cmd')
    try {
        @(
            '@echo off',
            "call `"$VsDevCmd`" -no_logo -arch=x64 -host_arch=x64",
            'if errorlevel 1 exit /b %errorlevel%',
            'set'
        ) | Set-Content -LiteralPath $batch -Encoding Ascii
        $capture = Invoke-NativeCapture $env:ComSpec '/d' '/c' $batch
    } finally {
        Remove-Item -LiteralPath $batch -Force -ErrorAction SilentlyContinue
    }
    if ($capture.ExitCode -ne 0) { throw 'Falha ao carregar o ambiente MSVC x64.' }
    foreach ($line in ($capture.Output -split '\r?\n')) {
        $separator = $line.IndexOf('=')
        if ($separator -le 0) { continue }
        $name = $line.Substring(0, $separator)
        $value = $line.Substring($separator + 1)
        [Environment]::SetEnvironmentVariable($name, $value, 'Process')
    }
}

function Test-Health {
    try {
        $response = Invoke-WebRequest -Uri $script:HealthUrl -UseBasicParsing -TimeoutSec 2
        if ($response.StatusCode -ne 200 -or
            -not ([string]$response.Headers['Content-Type']).StartsWith('application/json', [StringComparison]::OrdinalIgnoreCase) -or
            -not ([string]$response.Headers['Cache-Control']).Contains('no-store')) {
            return $false
        }
        $payload = $response.Content | ConvertFrom-Json
        return $payload.status -eq 'ok'
    } catch {
        return $false
    }
}

function Test-LoopbackPortOpen {
    $client = [Net.Sockets.TcpClient]::new()
    try {
        $connection = $client.ConnectAsync('127.0.0.1', 8742)
        return $connection.Wait(500) -and $client.Connected
    } catch {
        return $false
    } finally {
        $client.Dispose()
    }
}

function Get-DevelopmentServerProcesses {
    if (-not (Test-Path -LiteralPath $script:DevelopmentServerExecutable -PathType Leaf)) {
        return @()
    }
    return @(Get-CimInstance Win32_Process -ErrorAction Stop | Where-Object {
        $_.Name -eq "$ServerBinaryName.exe" -and
        [string]::Equals($_.ExecutablePath, $script:DevelopmentServerExecutable, [StringComparison]::OrdinalIgnoreCase) -and
        $_.CommandLine -match '(?i)(?:^|\s)--console(?:\s|$)' -and
        $_.CommandLine.IndexOf($script:DevelopmentData, [StringComparison]::OrdinalIgnoreCase) -ge 0
    })
}

function Open-ExistingDevelopmentServer {
    $developmentProcesses = @(Get-DevelopmentServerProcesses)
    if (Test-Health) {
        if ($developmentProcesses.Count -ne 1) {
            throw 'A porta administrativa pertence a outro processo ou serviço. Interrompa-o explicitamente antes de iniciar o ambiente de desenvolvimento.'
        }
        Write-Host '[OK] A instância de desenvolvimento esperada já está saudável.' -ForegroundColor Green
        Start-Process $script:ApplicationUrl
        Write-Host "`nSUCESSO: sistema disponível em $script:ApplicationUrl" -ForegroundColor Green
        return $true
    }
    if ($developmentProcesses.Count -gt 0) {
        throw 'A instância de desenvolvimento esperada está ativa, mas não está saudável. Encerre-a explicitamente e consulte os logs antes de recompilar.'
    }
    if (Test-LoopbackPortOpen) {
        throw 'A porta 8742 já está ocupada por um processo que não respondeu ao health check esperado.'
    }
    return $false
}

Write-Step 'Validando sistema operacional'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw 'Este script só pode ser executado no Windows.'
}
if (-not [Environment]::Is64BitOperatingSystem) { throw 'Windows x64 é obrigatório.' }
$windowsBuild = [Environment]::OSVersion.Version.Build
if ($windowsBuild -lt 22000) { throw "Windows 11 é obrigatório; build detectada: $windowsBuild." }
Write-Host "[OK] Windows 11 x64 (build $windowsBuild)" -ForegroundColor Green

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
try {
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Não execute este iniciador em um PowerShell elevado. O winget solicitará elevação apenas para instaladores que precisarem dela.'
    }
} finally {
    $identity.Dispose()
}

# Console mode is deliberately isolated from the service's protected
# %ProgramData% root. Its product root is the parent of this absolute Data path.
$programDataRoot = [IO.Path]::GetFullPath($env:ProgramData).TrimEnd('\') + '\'
if ($script:DevelopmentRoot.StartsWith($programDataRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'O modo console de desenvolvimento não pode usar um diretório sob ProgramData.'
}
$developmentRootPath = [IO.Path]::GetPathRoot($script:DevelopmentRoot)
if (-not $developmentRootPath -or $script:DevelopmentRoot.StartsWith('\\', [StringComparison]::Ordinal)) {
    throw 'O modo console de desenvolvimento exige um caminho local absoluto.'
}
$developmentDrive = [IO.DriveInfo]::new($developmentRootPath)
if (-not $developmentDrive.IsReady -or $developmentDrive.DriveType -ne [IO.DriveType]::Fixed) {
    throw 'O banco de desenvolvimento só pode usar uma unidade local fixa; SMB, UNC e unidades mapeadas são proibidos.'
}
$existingDevelopmentAncestor = $script:DevelopmentRoot
while (-not (Test-Path -LiteralPath $existingDevelopmentAncestor -PathType Container)) {
    $existingDevelopmentAncestor = Split-Path -Parent $existingDevelopmentAncestor
    if (-not $existingDevelopmentAncestor) { throw 'Não foi possível validar o caminho de desenvolvimento.' }
}
$ancestor = Get-Item -LiteralPath $existingDevelopmentAncestor
while ($null -ne $ancestor) {
    if (($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "O caminho de desenvolvimento atravessa um reparse point não permitido: '$($ancestor.FullName)'."
    }
    $ancestor = $ancestor.Parent
}
Write-Host "[OK] Dados de desenvolvimento isolados em '$script:DevelopmentData'" -ForegroundColor Green

if (Open-ExistingDevelopmentServer) { exit 0 }

Refresh-ProcessPath

Write-Step 'Validando Node.js e npm'
$nodeVersion = Get-SemanticVersion 'node.exe' @('--version')
$expectedNodeVersion = [version]$script:ExpectedNode
Assert-OrInstall `
    ($null -ne $nodeVersion -and $nodeVersion.Major -eq 24 -and $nodeVersion -eq $expectedNodeVersion) `
    "Node.js $script:ExpectedNode" `
    { Install-WingetPackage -Id 'OpenJS.NodeJS.LTS' -Version $script:ExpectedNode -Force }
Refresh-ProcessPath
$nodeVersion = Get-SemanticVersion 'node.exe' @('--version')
if ($null -eq $nodeVersion -or $nodeVersion -ne $expectedNodeVersion) {
    throw "Node.js $script:ExpectedNode continua indisponível após a validação."
}
$npmVersion = Get-SemanticVersion 'npm.cmd' @('--version')
Assert-OrInstall `
    ($null -ne $npmVersion -and $npmVersion.Major -eq 11) `
    'npm 11' `
    { Install-WingetPackage -Id 'OpenJS.NodeJS.LTS' -Version $script:ExpectedNode -Force }
$npmVersion = Get-SemanticVersion 'npm.cmd' @('--version')
if ($null -eq $npmVersion -or $npmVersion.Major -ne 11) { throw 'npm 11 não está disponível.' }

Write-Step 'Validando Rust MSVC fixado'
Assert-OrInstall (Test-Command 'rustup.exe') 'rustup' {
    Install-WingetPackage -Id 'Rustlang.Rustup'
}
Refresh-ProcessPath
if ($InstallMissing) {
    Invoke-Checked 'rustup.exe' 'toolchain' 'install' $script:ExpectedRust '--profile' 'minimal' '--component' 'rustfmt' '--component' 'clippy' '--target' 'x86_64-pc-windows-msvc'
}
$rustCapture = if (Test-Command 'rustc.exe') {
    Invoke-NativeCapture 'rustc.exe' "+$script:ExpectedRust" '--version' '--verbose'
} else {
    $null
}
$rustVerbose = if ($null -ne $rustCapture) { $rustCapture.Output } else { $null }
$rustOk = $null -ne $rustCapture -and $rustCapture.ExitCode -eq 0 -and ($rustVerbose -match "release: $([regex]::Escape($script:ExpectedRust))") -and ($rustVerbose -match 'host: x86_64-pc-windows-msvc')
if (-not $rustOk) {
    throw "Rust $script:ExpectedRust para x86_64-pc-windows-msvc não está instalado. Execute com -InstallMissing."
}
Write-Host "[OK] Rust $script:ExpectedRust MSVC" -ForegroundColor Green

Write-Step 'Validando Visual Studio C++ e Windows SDK'
$vsDevCmd = Find-VsDevCmd
$vsReady = $null -ne $vsDevCmd -and (Test-WindowsSdk)
Assert-OrInstall $vsReady 'Visual Studio 2022 Build Tools (C++/Windows SDK)' {
    Install-WingetPackage `
        -Id 'Microsoft.VisualStudio.2022.BuildTools' `
        -Override '--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended' `
        -Force
}
$vsDevCmd = Find-VsDevCmd
if ($null -eq $vsDevCmd -or -not (Test-WindowsSdk)) {
    throw 'Visual Studio 2022 Build Tools com C++ e Windows SDK continua indisponível.'
}
Import-VsDeveloperEnvironment $vsDevCmd
Write-Host '[OK] MSVC x64 e Windows SDK' -ForegroundColor Green

Write-Step 'Validando Perl e NASM para SQLCipher/OpenSSL'
Assert-OrInstall (Test-Command 'perl.exe') 'Perl' {
    Install-WingetPackage -Id 'StrawberryPerl.StrawberryPerl'
}
Assert-OrInstall (Test-Command 'nasm.exe') 'NASM' {
    Install-WingetPackage -Id 'NASM.NASM'
}
Refresh-ProcessPath
Import-VsDeveloperEnvironment $vsDevCmd
if (-not (Test-Command 'perl.exe') -or -not (Test-Command 'nasm.exe')) {
    throw 'Perl/NASM continuam indisponíveis após a validação.'
}
Write-Host '[OK] Perl e NASM' -ForegroundColor Green

Push-Location $script:RepositoryRoot
try {
    Write-Step 'Instalando dependências frontend do lockfile'
    Invoke-Checked 'npm.cmd' 'ci'

    Write-Step 'Validando frontend'
    Invoke-Checked 'npm.cmd' 'run' 'format:check'
    Invoke-Checked 'npm.cmd' 'run' 'lint'
    Invoke-Checked 'npm.cmd' 'run' 'typecheck'
    if (-not $SkipTests) { Invoke-Checked 'npm.cmd' 'test' '--' '--run' }
    Invoke-Checked 'npm.cmd' 'run' 'build'
    Invoke-Checked 'npm.cmd' 'run' 'verify:pwa'

    Write-Step 'Validando e compilando servidor Rust'
    Invoke-Checked 'cargo.exe' "+$script:ExpectedRust" 'fmt' '--manifest-path' $script:ManifestPath '--all' '--' '--check'
    Invoke-Checked 'cargo.exe' "+$script:ExpectedRust" 'clippy' '--manifest-path' $script:ManifestPath '--locked' '--all-targets' '--all-features' '--' '-D' 'warnings'
    if (-not $SkipTests) {
        Invoke-Checked 'cargo.exe' "+$script:ExpectedRust" 'test' '--manifest-path' $script:ManifestPath '--locked' '--all-features'
    }
    Invoke-Checked 'cargo.exe' "+$script:ExpectedRust" 'build' '--manifest-path' $script:ManifestPath '--locked' '--all-features'

    if (-not (Test-Path -LiteralPath $script:DevelopmentServerExecutable -PathType Leaf)) {
        throw "Executável do servidor não foi gerado em '$script:DevelopmentServerExecutable'."
    }

    if ((Test-Health) -or (Test-LoopbackPortOpen)) {
        throw 'A porta 8742 foi ocupada durante o build; a inicialização foi interrompida para não selecionar outra instância.'
    }
    Write-Step 'Iniciando servidor web local'
    $logDirectory = Join-Path $script:RepositoryRoot '.local-data\logs'
    New-Item -ItemType Directory -Force -Path $script:DevelopmentData, $logDirectory | Out-Null
    $stdout = Join-Path $logDirectory 'web-host.stdout.log'
    $stderr = Join-Path $logDirectory 'web-host.stderr.log'
    $quotedDevelopmentData = '"' + $script:DevelopmentData.Replace('"', '\"') + '"'
    $process = Start-Process `
        -FilePath $script:DevelopmentServerExecutable `
        -ArgumentList @('--console', '--data-directory', $quotedDevelopmentData) `
        -WorkingDirectory $script:RepositoryRoot `
        -RedirectStandardOutput $stdout `
        -RedirectStandardError $stderr `
        -PassThru

    $ready = $false
    $deadline = [DateTime]::UtcNow.AddSeconds(45)
    while ([DateTime]::UtcNow -lt $deadline) {
        if ($process.HasExited) { break }
        if (Test-Health) {
            $ready = $true
            break
        }
        Start-Sleep -Milliseconds 500
    }
    if (-not $ready) {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -ErrorAction SilentlyContinue }
        throw "O servidor não respondeu em $script:HealthUrl. Consulte '$stderr'."
    }

    Write-Step 'Abrindo Offline Dental System'
    Start-Process $script:ApplicationUrl
    Write-Host "`nSUCESSO: sistema disponível em $script:ApplicationUrl" -ForegroundColor Green
} finally {
    Pop-Location
}
