[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$serviceName = 'OfflineDentalSystem'
$productRoot = Join-Path $env:ProgramData 'OfflineDentalSystem'
$installRoot = Join-Path $env:ProgramFiles 'Offline Dental System'
$firewallNames = @(
    'Offline Dental System HTTPS (Private-Domain)',
    'Offline Dental System mDNS (Private-Domain)',
    'Offline Dental System HTTPS (Private)',
    'Offline Dental System HTTPS (Domain)',
    'Offline Dental System mDNS (Private)',
    'Offline Dental System mDNS (Domain)'
)

$service = Get-CimInstance Win32_Service -Filter "Name='$serviceName'" -ErrorAction SilentlyContinue
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

[pscustomobject]@{
    service = if ($service) {
        [pscustomobject]@{
            name = $service.Name
            state = $service.State
            startMode = $service.StartMode
            account = $service.StartName
            pid = $service.ProcessId
            imagePath = $service.PathName
        }
    } else { $null }
    installRootExists = Test-Path -LiteralPath $installRoot
    productRootExists = Test-Path -LiteralPath $productRoot
    ports = $ports
    firewall = $firewall
    certificates = $certificates
    health = $health
    runtimeLogPaths = @(Get-ChildItem -LiteralPath (Join-Path $productRoot 'logs\runtime') -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 10 -ExpandProperty FullName)
} | ConvertTo-Json -Depth 6
