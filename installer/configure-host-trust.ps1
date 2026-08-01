[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ProductRoot
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$healthUrl = 'http://127.0.0.1:8742/api/v1/health'
$caPath = Join-Path $ProductRoot 'tls\ca.cer'
$deadline = [DateTime]::UtcNow.AddSeconds(60)
$healthy = $false

while ([DateTime]::UtcNow -lt $deadline) {
    try {
        $response = Invoke-WebRequest -Uri $healthUrl -UseBasicParsing -TimeoutSec 2
        if ($response.StatusCode -eq 200 -and (Test-Path -LiteralPath $caPath -PathType Leaf)) {
            $healthy = $true
            break
        }
    } catch {
        # Service startup is asynchronous; retry until the fixed deadline.
    }
    Start-Sleep -Milliseconds 500
}

if (-not $healthy) {
    throw 'O serviço não gerou a identidade TLS dentro do prazo seguro.'
}

$certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new($caPath)
try {
    $basicConstraints = $certificate.Extensions |
        Where-Object { $_ -is [Security.Cryptography.X509Certificates.X509BasicConstraintsExtension] } |
        Select-Object -First 1
    if ($null -eq $basicConstraints -or -not $basicConstraints.CertificateAuthority) {
        throw 'O certificado gerado não é uma autoridade certificadora válida.'
    }
    if ($certificate.NotBefore.ToUniversalTime() -gt [DateTime]::UtcNow.AddMinutes(10) -or
        $certificate.NotAfter.ToUniversalTime() -le [DateTime]::UtcNow.AddDays(30)) {
        throw 'A validade da autoridade certificadora gerada é inválida.'
    }
    if (-not $certificate.Subject.StartsWith('CN=Offline Dental System CA ', [StringComparison]::Ordinal)) {
        throw 'A identidade da autoridade certificadora gerada é inválida.'
    }

    $store = [Security.Cryptography.X509Certificates.X509Store]::new(
        [Security.Cryptography.X509Certificates.StoreName]::Root,
        [Security.Cryptography.X509Certificates.StoreLocation]::LocalMachine
    )
    $store.Open([Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
    try {
        $existing = $store.Certificates.Find(
            [Security.Cryptography.X509Certificates.X509FindType]::FindByThumbprint,
            $certificate.Thumbprint,
            $false
        )
        if ($existing.Count -eq 0) {
            $store.Add($certificate)
        }
    } finally {
        $store.Close()
    }
} finally {
    $certificate.Dispose()
}
