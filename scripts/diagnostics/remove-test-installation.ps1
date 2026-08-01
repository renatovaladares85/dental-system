[CmdletBinding(SupportsShouldProcess)]
param([switch]$PurgeData)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$serviceName = 'OfflineDentalSystem'
$installRoot = [IO.Path]::GetFullPath((Join-Path $env:ProgramFiles 'Offline Dental System'))
$productRoot = [IO.Path]::GetFullPath((Join-Path $env:ProgramData 'OfflineDentalSystem'))
$firewallNames = @(
    'Offline Dental System HTTPS (Private-Domain)',
    'Offline Dental System mDNS (Private-Domain)',
    'Offline Dental System HTTPS (Private)',
    'Offline Dental System HTTPS (Domain)',
    'Offline Dental System mDNS (Private)',
    'Offline Dental System mDNS (Domain)'
)

$service = Get-CimInstance Win32_Service -Filter "Name='$serviceName'" -ErrorAction SilentlyContinue
if ($service -and -not $service.PathName.StartsWith(('"' + $installRoot), [StringComparison]::OrdinalIgnoreCase)) {
    throw 'O serviço com o nome esperado possui ImagePath divergente; a remoção foi recusada.'
}
if ($PurgeData -and (Test-Path -LiteralPath $productRoot)) {
    $confirmation = Read-Host "Dados persistentes serão apagados. Digite APAGAR-DADOS para confirmar"
    if ($confirmation -cne 'APAGAR-DADOS') { throw 'PurgeData cancelado; nenhum dado foi removido.' }
}

if ($service) {
    if ($service.State -ne 'Stopped' -and $PSCmdlet.ShouldProcess($serviceName, 'Parar serviço de teste')) {
        Stop-Service -Name $serviceName -ErrorAction Stop
    }
    if ($PSCmdlet.ShouldProcess($serviceName, 'Remover serviço de teste')) {
        & (Join-Path $env:SystemRoot 'System32\sc.exe') delete $serviceName
        if ($LASTEXITCODE -ne 0) { throw "sc.exe delete falhou com código $LASTEXITCODE." }
    }
}

foreach ($name in $firewallNames) {
    if ($PSCmdlet.ShouldProcess($name, 'Remover regra de firewall exata')) {
        Remove-NetFirewallRule -DisplayName $name -ErrorAction SilentlyContinue
    }
}

$caPath = Join-Path $productRoot 'tls\ca.cer'
if (Test-Path -LiteralPath $caPath -PathType Leaf) {
    $ca = [Security.Cryptography.X509Certificates.X509Certificate2]::new($caPath)
    try {
        if ($ca.Subject -like 'CN=Offline Dental System CA*' -and
            $PSCmdlet.ShouldProcess($ca.Thumbprint, 'Remover CA pertencente à instalação')) {
            Get-ChildItem Cert:\LocalMachine\Root | Where-Object { $_.Thumbprint -eq $ca.Thumbprint } |
                Remove-Item -Force
        }
    } finally {
        $ca.Dispose()
    }
}

if ((Test-Path -LiteralPath $installRoot) -and $PSCmdlet.ShouldProcess($installRoot, 'Remover payload do produto')) {
    Remove-Item -LiteralPath $installRoot -Recurse -Force
}
if ($PurgeData -and (Test-Path -LiteralPath $productRoot) -and
    $PSCmdlet.ShouldProcess($productRoot, 'Remover dados persistentes confirmados')) {
    Remove-Item -LiteralPath $productRoot -Recurse -Force
}
