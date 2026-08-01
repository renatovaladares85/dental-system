[CmdletBinding()]
param(
    [switch]$RemoveData,
    [switch]$PlanOnly,
    [switch]$Elevated,
    [string]$RunId,
    [string]$LogPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$utf8 = [Text.UTF8Encoding]::new($false)
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8

$serviceName = 'OfflineDentalSystem'
$serviceModule = Join-Path $PSScriptRoot 'lib\windows-service.psm1'
if (-not (Test-Path -LiteralPath $serviceModule -PathType Leaf)) {
    throw "Módulo obrigatório de serviço Windows ausente: '$serviceModule'."
}
Import-Module -Name $serviceModule -Force
$registryPath = 'HKLM:\Software\RenatoValadares\OfflineDentalSystem'
$defaultInstallRoot = Join-Path $env:ProgramFiles 'Offline Dental System'
$defaultDataRoot = Join-Path $env:ProgramData 'OfflineDentalSystem'
$firewallRules = @(
    'Offline Dental System HTTPS (Private-Domain)',
    'Offline Dental System mDNS (Private-Domain)',
    'Offline Dental System HTTPS (Private)',
    'Offline Dental System HTTPS (Domain)',
    'Offline Dental System mDNS (Private)',
    'Offline Dental System mDNS (Domain)'
)

if (-not $RunId) { $RunId = [guid]::NewGuid().ToString('N') }
if ($RunId -notmatch '^[0-9a-f]{32}$') { throw 'RunId de desinstalação inválido.' }
if (-not $LogPath) {
    $logDirectory = Join-Path $env:LOCALAPPDATA 'OfflineDentalSystem\Logs\installation'
    New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
    $LogPath = Join-Path $logDirectory ("uninstall-$([DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'))-$RunId.jsonl")
}
$LogPath = [IO.Path]::GetFullPath($LogPath)
$allowedLogRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'OfflineDentalSystem\Logs')).TrimEnd('\') + '\'
if (-not $LogPath.StartsWith($allowedLogRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Caminho de log inválido.' }
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $LogPath) | Out-Null

function Write-UninstallEvent {
    param(
        [Parameter(Mandatory)][ValidateSet('INFO', 'WARN', 'ERROR')][string]$Level,
        [Parameter(Mandatory)][string]$Event,
        [Parameter(Mandatory)][string]$Message,
        [hashtable]$Data = @{}
    )
    $record = [ordered]@{
        timestampUtc = [DateTime]::UtcNow.ToString('o')
        runId = $RunId
        processId = $PID
        elevated = [bool]$Elevated
        level = $Level
        event = $Event
        message = $Message
        data = $Data
    }
    [IO.File]::AppendAllText(
        $LogPath,
        (($record | ConvertTo-Json -Compress -Depth 5) + [Environment]::NewLine),
        [Text.UTF8Encoding]::new($false)
    )
}

function Test-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Get-InstalledPaths {
    $installRoot = $defaultInstallRoot
    $dataRoot = $defaultDataRoot
    if (Test-Path -LiteralPath $registryPath) {
        $properties = Get-ItemProperty -LiteralPath $registryPath
        if ($properties.InstallRoot) { $installRoot = [IO.Path]::GetFullPath([string]$properties.InstallRoot) }
        if ($properties.DataRoot) { $dataRoot = [IO.Path]::GetFullPath([string]$properties.DataRoot) }
    }
    return [pscustomobject]@{ InstallRoot = $installRoot; DataRoot = $dataRoot }
}

function Assert-FixedPath([string]$Actual, [string]$Expected, [string]$Description) {
    $actualFull = [IO.Path]::GetFullPath($Actual).TrimEnd('\')
    $expectedFull = [IO.Path]::GetFullPath($Expected).TrimEnd('\')
    if (-not $actualFull.Equals($expectedFull, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Description recusado porque o caminho instalado não corresponde ao caminho controlado."
    }
    if (Test-Path -LiteralPath $actualFull) {
        $item = Get-Item -LiteralPath $actualFull -Force
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw "$Description recusado porque o diretório é um link, junction ou reparse point."
        }
    }
}

function Invoke-Sc([string[]]$Arguments, [int[]]$AcceptedCodes = @(0)) {
    $sc = Join-Path $env:SystemRoot 'System32\sc.exe'
    $eventWriter = {
        param($Level, $Event, $Message, $Data)
        Write-UninstallEvent -Level $Level -Event $Event -Message $Message -Data $Data
    }
    Invoke-OdsNativeProcess -FilePath $sc -Arguments $Arguments -AcceptedExitCodes $AcceptedCodes `
        -EventWriter $eventWriter -EventData @{
            operation = if ($Arguments.Count -gt 0) { $Arguments[0] } else { 'unknown' }
            serviceName = $serviceName
        } | Out-Null
}

function Remove-InstalledCa([string]$DataRoot) {
    $caPath = Join-Path $DataRoot 'tls\ca.cer'
    if (-not (Test-Path -LiteralPath $caPath -PathType Leaf)) { return }
    $certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new($caPath)
    try {
        $store = [Security.Cryptography.X509Certificates.X509Store]::new(
            [Security.Cryptography.X509Certificates.StoreName]::Root,
            [Security.Cryptography.X509Certificates.StoreLocation]::LocalMachine
        )
        $store.Open([Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
        try {
            $matches = $store.Certificates.Find(
                [Security.Cryptography.X509Certificates.X509FindType]::FindByThumbprint,
                $certificate.Thumbprint,
                $false
            )
            foreach ($match in $matches) { $store.Remove($match) }
        } finally {
            $store.Close()
        }
    } finally {
        $certificate.Dispose()
    }
}

try {
    Write-UninstallEvent -Level INFO -Event 'UNINSTALL_STARTED' -Message 'Desinstalação iniciada.' -Data @{ removeData = [bool]$RemoveData; planOnly = [bool]$PlanOnly }
    $paths = Get-InstalledPaths
    Assert-FixedPath $paths.InstallRoot $defaultInstallRoot 'InstallRoot'
    Assert-FixedPath $paths.DataRoot $defaultDataRoot 'DataRoot'

    if ($PlanOnly) {
        [pscustomobject]@{
            service = $serviceName
            installRoot = $paths.InstallRoot
            dataRoot = $paths.DataRoot
            removeData = [bool]$RemoveData
        } | ConvertTo-Json
        Write-UninstallEvent -Level INFO -Event 'UNINSTALL_PLAN_COMPLETED' -Message 'Plano validado sem alterar o computador.'
        exit 0
    }

    $installationEvidence = (Test-Path -LiteralPath $registryPath) -or
        ($null -ne (Get-Service -Name $serviceName -ErrorAction SilentlyContinue))
    if (-not $installationEvidence) {
        Write-Host 'O Offline Dental System já não está instalado. Nenhum arquivo ou dado foi alterado.' -ForegroundColor Green
        Write-UninstallEvent -Level INFO -Event 'UNINSTALL_ALREADY_ABSENT' -Message 'Nenhuma instalação foi encontrada.'
        exit 0
    }

    if (-not $Elevated -and -not (Test-Administrator)) {
        Write-UninstallEvent -Level INFO -Event 'UNINSTALL_ELEVATION_REQUESTED' -Message 'Solicitando privilégios administrativos.'
        $arguments = @(
            '-NoLogo', '-NoProfile', '-ExecutionPolicy', 'Bypass',
            '-File', $PSCommandPath, '-Elevated',
            '-RunId', $RunId,
            '-LogPath', $LogPath
        )
        if ($RemoveData) { $arguments += '-RemoveData' }
        try {
            $process = Start-OdsProcess -FilePath 'powershell.exe' -Arguments $arguments -Verb RunAs -Wait -PassThru
        } catch [ComponentModel.Win32Exception] {
            if ($_.Exception.NativeErrorCode -eq 1223) { throw 'Desinstalação cancelada na confirmação de segurança do Windows.' }
            throw
        }
        if ($process.ExitCode -ne 0) {
            $lastError = Get-Content -Encoding UTF8 -LiteralPath $LogPath -ErrorAction SilentlyContinue |
                ForEach-Object { try { $_ | ConvertFrom-Json } catch { $null } } |
                Where-Object { $_.level -eq 'ERROR' } |
                Select-Object -Last 1
            $detail = if ($lastError -and $lastError.message) { [string]$lastError.message } else { 'Falha administrativa sem detalhe adicional.' }
            throw "$detail Consulte o log '$LogPath'."
        }
        Write-Host "Log: $LogPath"
        exit 0
    }

    if (-not (Test-Administrator)) { throw 'A desinstalação requer privilégios de administrador.' }

    if ($RemoveData) {
        Write-Host ''
        Write-Host 'ATENÇÃO: banco, chaves, configurações e dados locais serão apagados permanentemente.' -ForegroundColor Red
        $confirmation = Read-Host 'Digite REMOVER para confirmar'
        if ($confirmation -cne 'REMOVER') {
            throw 'Remoção completa cancelada; nenhum dado foi apagado.'
        }
        Write-UninstallEvent -Level WARN -Event 'DATA_REMOVAL_CONFIRMED' -Message 'Remoção integral dos dados confirmada explicitamente.'
    }

    $service = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
    if ($null -ne $service) {
        if ($service.Status -ne 'Stopped') {
            Invoke-Sc @('stop', $serviceName) @(0, 1062)
            $service.WaitForStatus([ServiceProcess.ServiceControllerStatus]::Stopped, [TimeSpan]::FromSeconds(30))
        }
        Invoke-Sc @('delete', $serviceName) @(0, 1060, 1072)
    }

    foreach ($rule in $firewallRules) {
        Remove-NetFirewallRule -DisplayName $rule -ErrorAction SilentlyContinue
    }
    Remove-InstalledCa $paths.DataRoot

    $desktopShortcut = Join-Path $env:PUBLIC 'Desktop\Offline Dental System.lnk'
    $startMenu = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\Offline Dental System'
    Remove-Item -LiteralPath $desktopShortcut -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $startMenu -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $registryPath -Recurse -Force -ErrorAction SilentlyContinue

    Set-Location -LiteralPath $env:TEMP
    [Environment]::CurrentDirectory = $env:TEMP
    if (Test-Path -LiteralPath $paths.InstallRoot) {
        Remove-Item -LiteralPath $paths.InstallRoot -Recurse -Force
    }
    if ($RemoveData -and (Test-Path -LiteralPath $paths.DataRoot)) {
        Remove-Item -LiteralPath $paths.DataRoot -Recurse -Force
    }

    if ($RemoveData) {
        Write-Host 'SUCESSO: aplicação e dados locais foram removidos.' -ForegroundColor Green
    } else {
        Write-Host "SUCESSO: aplicação removida. Dados preservados em '$($paths.DataRoot)'." -ForegroundColor Green
    }
    Write-UninstallEvent -Level INFO -Event 'UNINSTALL_COMPLETED' -Message 'Desinstalação concluída.' -Data @{ dataRemoved = [bool]$RemoveData }
    Write-Host "Log: $LogPath"
    exit 0
} catch {
    try {
        Write-UninstallEvent -Level ERROR -Event 'UNINSTALL_FAILED' -Message $_.Exception.Message -Data @{
            exceptionType = $_.Exception.GetType().FullName
            line = $_.InvocationInfo.ScriptLineNumber
        }
    } catch {
        # Não substitui a falha original quando o log também estiver indisponível.
    }
    Write-Host ''
    Write-Host 'ERRO:' $_.Exception.Message -ForegroundColor Red
    Write-Host "Log: $LogPath" -ForegroundColor Yellow
    exit 1
}
