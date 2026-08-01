[CmdletBinding()]
param([switch]$UnitTest)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$serviceName = 'OfflineDentalSystem'
$serviceRegistryPath = "HKLM:\SYSTEM\CurrentControlSet\Services\$serviceName"
$sc = Join-Path $env:SystemRoot 'System32\sc.exe'

function Get-OdsExpectedServiceImagePath {
    param([Parameter(Mandatory)][string]$InstallDirectory)

    $expectedExecutable = [IO.Path]::GetFullPath(
        (Join-Path $InstallDirectory 'offline-dental-system.exe')
    )
    return '"' + $expectedExecutable + '" --service'
}

function Test-OdsServiceImagePath {
    param(
        [Parameter(Mandatory)][string]$ActualImagePath,
        [Parameter(Mandatory)][string]$ExpectedExecutable
    )

    $expected = '"' + [IO.Path]::GetFullPath($ExpectedExecutable) + '" --service'
    return $ActualImagePath.Trim().Equals($expected, [StringComparison]::OrdinalIgnoreCase)
}

if ($UnitTest) { return }

if (-not (Test-Path -LiteralPath $sc -PathType Leaf)) {
    throw 'O Service Control Manager não está disponível.'
}
if (-not (Get-Service -Name $serviceName -ErrorAction SilentlyContinue)) {
    throw 'O serviço Offline Dental System ainda não foi instalado.'
}

function Invoke-ServiceControl([string[]]$Arguments) {
    $output = & $sc @Arguments 2>&1
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        $diagnostic = ($output | Out-String).Trim()
        if ($diagnostic) { Write-Verbose $diagnostic }
        throw "Falha ao configurar o serviço (código $exitCode)."
    }
}

# Do this before StartServices. The restricted service SID is the only
# non-administrative principal granted access to the persistent data root.
Invoke-ServiceControl @('sidtype', $serviceName, 'restricted')
Invoke-ServiceControl @('config', $serviceName, 'start=', 'delayed-auto')

$configuration = Get-ItemProperty -LiteralPath $serviceRegistryPath
if ([int]$configuration.Start -ne 2 -or
    [int]$configuration.DelayedAutoStart -ne 1 -or
    [int]$configuration.ServiceSidType -ne 3) {
    throw 'A configuração restrita do serviço não foi aplicada integralmente.'
}

$service = Get-CimInstance Win32_Service -Filter "Name='$serviceName'" -ErrorAction SilentlyContinue
if ($null -eq $service) {
    throw 'O serviço Offline Dental System não existe após a configuração.'
}
if ($service.StartName -ne 'NT AUTHORITY\LocalService') {
    throw 'A conta persistida do serviço não é NT AUTHORITY\LocalService.'
}
if ($service.StartMode -ne 'Auto') {
    throw 'O serviço não está configurado para início automático.'
}
$expectedExecutable = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'offline-dental-system.exe'))
if (-not (Test-Path -LiteralPath $expectedExecutable -PathType Leaf)) {
    throw 'O executável esperado do serviço não existe no diretório instalado.'
}
$actualImagePath = ([string]$service.PathName).Trim()
if (-not (Test-OdsServiceImagePath -ActualImagePath $actualImagePath -ExpectedExecutable $expectedExecutable)) {
    throw 'O ImagePath persistido do serviço diverge da configuração esperada.'
}
