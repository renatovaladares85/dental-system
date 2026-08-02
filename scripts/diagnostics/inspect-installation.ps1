[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$serviceName = 'OfflineDentalSystem'
$productRoot = Join-Path $env:ProgramData 'OfflineDentalSystem'
$installRoot = Join-Path $env:ProgramFiles 'Offline Dental System'
$serverExecutable = Join-Path $installRoot 'offline-dental-system.exe'
$serviceRegistryPath = "HKLM:\SYSTEM\CurrentControlSet\Services\$serviceName"
$firewallNames = @(
    'Offline Dental System HTTPS (Private-Domain)',
    'Offline Dental System mDNS (Private-Domain)',
    'Offline Dental System HTTPS (Private)',
    'Offline Dental System HTTPS (Domain)',
    'Offline Dental System mDNS (Private)',
    'Offline Dental System mDNS (Domain)'
)

$service = Get-CimInstance Win32_Service -Filter "Name='$serviceName'" -ErrorAction SilentlyContinue
$serviceRegistry = Get-ItemProperty -LiteralPath $serviceRegistryPath -ErrorAction SilentlyContinue
$ports = @(8742, 8743 | ForEach-Object {
    Get-NetTCPConnection -LocalPort $_ -State Listen -ErrorAction SilentlyContinue |
        Select-Object LocalAddress, LocalPort, OwningProcess
})
$firewall = @($firewallNames | ForEach-Object {
    Get-NetFirewallRule -DisplayName $_ -ErrorAction SilentlyContinue |
        Select-Object DisplayName, Enabled, Profile, Direction, Action
})
$certificates = @(Get-ChildItem Cert:\LocalMachine\Root -ErrorAction SilentlyContinue |
    Where-Object { $_.Subject -like 'CN=Offline Dental System CA*' } |
    Select-Object Subject, Thumbprint, NotBefore, NotAfter)
$health = try {
    $response = Invoke-WebRequest 'http://127.0.0.1:8742/api/v1/health' -TimeoutSec 2
    [pscustomobject]@{ statusCode = $response.StatusCode; contentType = $response.Headers['Content-Type']; cacheControl = $response.Headers['Cache-Control']; body = ($response.Content | ConvertFrom-Json).status }
} catch {
    [pscustomobject]@{ error = 'HEALTH_UNAVAILABLE' }
}
$startupDiagnostics = if (Test-Path -LiteralPath $serverExecutable -PathType Leaf) {
    try {
        $json = & $serverExecutable '--startup-diagnostics' '--json' 2>$null
        if ($LASTEXITCODE -ne 0) { throw 'diagnóstico indisponível' }
        $json | ConvertFrom-Json
    } catch {
        [pscustomobject]@{ error = 'STARTUP_DIAGNOSTICS_UNAVAILABLE' }
    }
} else {
    [pscustomobject]@{ error = 'EXECUTABLE_MISSING' }
}
$signature = if (Test-Path -LiteralPath $serverExecutable -PathType Leaf) {
    $authenticode = Get-AuthenticodeSignature -LiteralPath $serverExecutable
    [pscustomobject]@{
        status = $authenticode.Status.ToString()
        thumbprint = if ($authenticode.SignerCertificate) { $authenticode.SignerCertificate.Thumbprint } else { $null }
    }
} else { $null }
$runtimeLogs = @(Get-ChildItem -LiteralPath (Join-Path $productRoot 'logs\runtime') -File -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 10)

[pscustomobject]@{
    service = if ($service) {
        [pscustomobject]@{
            name = $service.Name
            state = $service.State
            startMode = $service.StartMode
            account = $service.StartName
            pid = $service.ProcessId
            imagePath = $service.PathName
            delayedAutoStart = if ($serviceRegistry) { $serviceRegistry.DelayedAutoStart } else { $null }
            serviceSidType = if ($serviceRegistry) { $serviceRegistry.ServiceSidType } else { $null }
            registryPath = $serviceRegistryPath
        }
    } else { $null }
    executable = if (Test-Path -LiteralPath $serverExecutable -PathType Leaf) {
        [pscustomobject]@{
            version = (Get-Item -LiteralPath $serverExecutable).VersionInfo.ProductVersion
            signature = $signature
        }
    } else { $null }
    installRootExists = Test-Path -LiteralPath $installRoot
    productRootExists = Test-Path -LiteralPath $productRoot
    ports = $ports
    firewall = $firewall
    certificates = $certificates
    health = $health
    startupDiagnostics = $startupDiagnostics
    runtimeLogs = @($runtimeLogs | Select-Object FullName, LastWriteTimeUtc)
} | ConvertTo-Json -Depth 6
