[CmdletBinding()]
param(
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
$healthUrl = 'http://127.0.0.1:8742/api/v1/health'
$applicationUrl = 'http://127.0.0.1:8742'

if (-not $RunId) { $RunId = [guid]::NewGuid().ToString('N') }
if ($RunId -notmatch '^[0-9a-f]{32}$') { throw 'RunId de abertura inválido.' }
if (-not $LogPath) {
    $logDirectory = Join-Path $env:LOCALAPPDATA 'OfflineDentalSystem\Logs\operation'
    New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
    $LogPath = Join-Path $logDirectory ("open-$([DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'))-$RunId.jsonl")
}
$LogPath = [IO.Path]::GetFullPath($LogPath)
$allowedLogRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'OfflineDentalSystem\Logs')).TrimEnd('\') + '\'
if (-not $LogPath.StartsWith($allowedLogRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Caminho de log inválido.' }
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $LogPath) | Out-Null

function Write-OpenEvent {
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

function Test-Health {
    try {
        $response = Invoke-WebRequest -Uri $healthUrl -UseBasicParsing -TimeoutSec 2
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

function Wait-Health([int]$TimeoutSeconds) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (Test-Health) { return $true }
        Start-Sleep -Milliseconds 500
    }
    return $false
}

try {
    Write-OpenEvent -Level INFO -Event 'OPEN_STARTED' -Message 'Abertura do sistema iniciada.'
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or
        -not [Environment]::Is64BitOperatingSystem -or
        [Environment]::OSVersion.Version.Build -lt 22000) {
        throw 'Este sistema requer Windows 11 x64.'
    }

    if (Test-Health) {
        Write-OpenEvent -Level INFO -Event 'OPEN_EXISTING_SERVER' -Message 'Servidor já estava disponível.'
        Start-Process $applicationUrl
        Write-Host "Log: $LogPath"
        exit 0
    }

    if ($null -eq (Get-Service -Name $serviceName -ErrorAction SilentlyContinue)) {
        throw 'O servidor local não está instalado. Execute Instalar-e-Iniciar.bat.'
    }

    $sc = Join-Path $env:SystemRoot 'System32\sc.exe'
    try {
        $process = Start-Process -FilePath $sc -ArgumentList @('start', $serviceName) -Verb RunAs -Wait -PassThru
    } catch [ComponentModel.Win32Exception] {
        if ($_.Exception.NativeErrorCode -eq 1223) {
            throw 'A inicialização foi cancelada na confirmação de segurança do Windows.'
        }
        throw
    }
    if ($process.ExitCode -notin @(0, 1056)) {
        throw "O Windows não conseguiu iniciar o serviço (código $($process.ExitCode))."
    }
    Write-OpenEvent -Level INFO -Event 'SERVICE_START_COMPLETED' -Message 'Comando de inicialização do serviço finalizado.' -Data @{ exitCode = $process.ExitCode }
    if (-not (Wait-Health 60)) {
        throw 'O serviço foi iniciado, mas o servidor web não respondeu. Execute Instalar-e-Iniciar.bat para reparar.'
    }

    Start-Process $applicationUrl
    Write-OpenEvent -Level INFO -Event 'OPEN_COMPLETED' -Message 'Health check aprovado e navegador solicitado.'
    Write-Host "Log: $LogPath"
    exit 0
} catch {
    try {
        Write-OpenEvent -Level ERROR -Event 'OPEN_FAILED' -Message $_.Exception.Message -Data @{
            exceptionType = $_.Exception.GetType().FullName
            line = $_.InvocationInfo.ScriptLineNumber
        }
    } catch {
        # Não substitui a causa original quando o log estiver indisponível.
    }
    Write-Host ''
    Write-Host 'ERRO:' $_.Exception.Message -ForegroundColor Red
    Write-Host "Log: $LogPath" -ForegroundColor Yellow
    exit 1
}
