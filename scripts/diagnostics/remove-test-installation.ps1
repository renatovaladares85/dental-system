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

$expectedServiceExecutable = Join-Path $installRoot 'offline-dental-system.exe'

function Find-OdsMsiInstallation {
    $uninstallRoot = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall'
    return Get-ChildItem -LiteralPath $uninstallRoot -ErrorAction SilentlyContinue |
        ForEach-Object { Get-ItemProperty -LiteralPath $_.PSPath } |
        Where-Object {
            $_.WindowsInstaller -eq 1 -and $_.DisplayName -eq 'Offline Dental System'
        } | Select-Object -First 1
}

function Test-OdsExpectedServiceImagePath([string]$ImagePath) {
    return $ImagePath -match ('(?i)^"' + [regex]::Escape($expectedServiceExecutable) + '"\s+--service\s*$')
}

function Assert-OdsTreeHasNoReparsePoints([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return }
    $items = @(Get-Item -LiteralPath $Path -Force) + @(Get-ChildItem -LiteralPath $Path -Force -Recurse)
    if ($items | Where-Object { ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 }) {
        throw "O caminho '$Path' contém reparse point; a remoção foi recusada."
    }
}

$msiInstallation = Find-OdsMsiInstallation
$service = Get-CimInstance Win32_Service -Filter "Name='$serviceName'" -ErrorAction SilentlyContinue
if ($service -and -not $msiInstallation -and -not (Test-OdsExpectedServiceImagePath $service.PathName)) {
    throw 'O serviço com o nome esperado possui ImagePath divergente; a remoção foi recusada.'
}
if ($PurgeData -and -not $WhatIfPreference -and (Test-Path -LiteralPath $productRoot)) {
    $confirmation = Read-Host "Dados persistentes serão apagados. Digite APAGAR-DADOS para confirmar"
    if ($confirmation -cne 'APAGAR-DADOS') { throw 'PurgeData cancelado; nenhum dado foi removido.' }
}

if ($msiInstallation) {
    $productCode = $msiInstallation.PSChildName
    if ($PSCmdlet.ShouldProcess($productCode, 'Desinstalar MSI de teste registrado')) {
        $process = Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\msiexec.exe') `
            -ArgumentList @('/x', $productCode, '/qn', '/norestart') -Wait -PassThru
        if ($process.ExitCode -notin @(0, 3010)) {
            throw "A desinstalação MSI falhou com código $($process.ExitCode)."
        }
    }
} elseif ($service) {
    if ($service.State -ne 'Stopped' -and $PSCmdlet.ShouldProcess($serviceName, 'Parar serviço de teste')) {
        Stop-Service -Name $serviceName -ErrorAction Stop
    }
    if ($PSCmdlet.ShouldProcess($serviceName, 'Remover serviço de teste')) {
        & (Join-Path $env:SystemRoot 'System32\sc.exe') delete $serviceName
        if ($LASTEXITCODE -ne 0) { throw "sc.exe delete falhou com código $LASTEXITCODE." }
    }
}

foreach ($name in $firewallNames) {
    $rules = @(Get-NetFirewallRule -DisplayName $name -ErrorAction SilentlyContinue |
        Where-Object { $_.DisplayName -ceq $name })
    foreach ($rule in $rules) {
        if ($PSCmdlet.ShouldProcess($name, 'Remover regra de firewall exata')) {
            Remove-NetFirewallRule -Name $rule.Name -ErrorAction Stop
        }
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

if (-not $msiInstallation -and (Test-Path -LiteralPath $installRoot)) {
    Assert-OdsTreeHasNoReparsePoints $installRoot
}
if (-not $msiInstallation -and (Test-Path -LiteralPath $installRoot) -and $PSCmdlet.ShouldProcess($installRoot, 'Remover payload do produto')) {
    Remove-Item -LiteralPath $installRoot -Recurse -Force
}
if ($PurgeData -and (Test-Path -LiteralPath $productRoot) -and
    $PSCmdlet.ShouldProcess($productRoot, 'Remover dados persistentes confirmados')) {
    Assert-OdsTreeHasNoReparsePoints $productRoot
    Remove-Item -LiteralPath $productRoot -Recurse -Force
}
