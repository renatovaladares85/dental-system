[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$serviceName = 'OfflineDentalSystem'
$serviceRegistryPath = "HKLM:\SYSTEM\CurrentControlSet\Services\$serviceName"
$sc = Join-Path $env:SystemRoot 'System32\sc.exe'

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
