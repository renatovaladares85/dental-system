[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$utf8 = [Text.UTF8Encoding]::new($false)
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8

$serviceName = 'OfflineDentalSystem'
$healthUrl = 'http://127.0.0.1:8742/api/v1/health'
$applicationUrl = 'http://127.0.0.1:8742'
$expectedPublisherThumbprint = '__ODS_SIGNING_CERT_THUMBPRINT__'
$expectedProductName = 'Offline Dental System'
$expectedManufacturer = 'Renato Valadares'
$expectedUpgradeCode = '{C31A25B0-5DEF-4497-8288-CA531C34D957}'
$acceptedInstallerExitCodes = @(0, 1641, 3010)

function Test-ApplicationHealth {
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

function Wait-ApplicationHealth([int]$TimeoutSeconds) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (Test-ApplicationHealth) { return $true }
        Start-Sleep -Milliseconds 500
    }
    return $false
}

function Open-Application {
    Start-Process $applicationUrl
    Write-Host 'Offline Dental System aberto no navegador.' -ForegroundColor Green
}

function Start-ElevatedProcess {
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [Parameter(Mandatory)][string[]]$Arguments
    )
    try {
        return Start-Process `
            -FilePath $FilePath `
            -ArgumentList $Arguments `
            -Verb RunAs `
            -Wait `
            -PassThru
    } catch [ComponentModel.Win32Exception] {
        if ($_.Exception.NativeErrorCode -eq 1223) {
            throw 'A operação foi cancelada na confirmação de segurança do Windows.'
        }
        throw
    }
}

function Try-StartInstalledService {
    $service = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
    if ($null -eq $service) { return $false }
    if ($service.Status -ne 'Running') {
        Write-Host 'Iniciando o serviço do Offline Dental System...'
        $sc = Join-Path $env:SystemRoot 'System32\sc.exe'
        $null = Start-ElevatedProcess -FilePath $sc -Arguments @('start', $serviceName)
    }
    return Wait-ApplicationHealth 60
}

function Find-InstallerPackage {
    $packages = @(Get-ChildItem -LiteralPath $PSScriptRoot -Filter 'OfflineDentalSystem-*-x64.msi' -File)
    if ($packages.Count -eq 0) {
        if ($expectedPublisherThumbprint -eq '__ODS_SIGNING_CERT_THUMBPRINT__') {
            throw 'Este launcher pertence ao código-fonte e ainda não possui um MSI assinado. Para testar localmente, execute scripts\start-windows.ps1 na raiz do projeto.'
        }
        throw 'O instalador MSI não foi encontrado. Extraia e mantenha todos os arquivos do pacote na mesma pasta.'
    }
    if ($packages.Count -ne 1) {
        throw 'Mais de um instalador MSI foi encontrado. Use um pacote limpo contendo somente uma versão.'
    }
    return $packages[0]
}

function Assert-InstallerSignature([string]$Path) {
    $expected = ($expectedPublisherThumbprint -replace '\s', '').ToUpperInvariant()
    if ($expected -notmatch '^[0-9A-F]{40}$') {
        throw 'Este launcher não foi preparado por um build de distribuição assinado.'
    }
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    $actual = if ($null -ne $signature.SignerCertificate) {
        (($signature.SignerCertificate.Thumbprint -replace '\s', '')).ToUpperInvariant()
    } else {
        ''
    }
    if ($signature.Status -ne 'Valid' -or $actual -ne $expected) {
        throw 'A assinatura digital do instalador é inválida ou pertence a outro fornecedor.'
    }
    if ($null -eq $signature.TimeStamperCertificate) {
        throw 'O instalador não possui timestamp digital verificável.'
    }
}

function Get-MsiProperty {
    param(
        [Parameter(Mandatory)]$Database,
        [Parameter(Mandatory)][string]$Name
    )
    $escapedName = $Name.Replace("'", "''")
    $view = $Database.OpenView("SELECT ``Value`` FROM ``Property`` WHERE ``Property`` = '$escapedName'")
    try {
        $view.Execute()
        $record = $view.Fetch()
        if ($null -eq $record) { return $null }
        try {
            return [string]$record.StringData(1)
        } finally {
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($record)
        }
    } finally {
        $view.Close()
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($view)
    }
}

function Assert-InstallerIdentity([string]$Path) {
    $windowsInstaller = New-Object -ComObject WindowsInstaller.Installer
    try {
        $database = $windowsInstaller.OpenDatabase($Path, 0)
        try {
            $productName = Get-MsiProperty -Database $database -Name 'ProductName'
            $manufacturer = Get-MsiProperty -Database $database -Name 'Manufacturer'
            $upgradeCode = Get-MsiProperty -Database $database -Name 'UpgradeCode'
            $productVersion = Get-MsiProperty -Database $database -Name 'ProductVersion'
            if ($productName -ne $expectedProductName -or
                $manufacturer -ne $expectedManufacturer -or
                $upgradeCode -ne $expectedUpgradeCode -or
                $productVersion -notmatch '^\d+\.\d+\.\d+$') {
                throw 'A identidade ou a versão do pacote MSI não corresponde ao Offline Dental System.'
            }
        } finally {
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($database)
        }
    } finally {
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($windowsInstaller)
    }
}

function Invoke-Installer {
    param(
        [Parameter(Mandatory)][string]$MsiPath,
        [Parameter(Mandatory)][bool]$Repair
    )
    $logDirectory = Join-Path $env:TEMP 'OfflineDentalSystem'
    New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
    $logPath = Join-Path $logDirectory 'installer.log'
    $quotedMsi = '"' + $MsiPath + '"'
    $quotedLog = '"' + $logPath + '"'
    $mode = if ($Repair) { '/fa' } else { '/i' }
    $arguments = @($mode, $quotedMsi, '/passive', '/norestart', 'REBOOT=ReallySuppress', '/l*v', $quotedLog)
    Write-Host ($(if ($Repair) { 'Reparando a instalação...' } else { 'Instalando o Offline Dental System...' }))
    $process = Start-ElevatedProcess -FilePath (Join-Path $env:SystemRoot 'System32\msiexec.exe') -Arguments $arguments
    if ($acceptedInstallerExitCodes -notcontains $process.ExitCode) {
        throw "O Windows Installer retornou o código $($process.ExitCode). Consulte '$logPath'."
    }
    return $process.ExitCode
}

try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or
        -not [Environment]::Is64BitOperatingSystem -or
        [Environment]::OSVersion.Version.Build -lt 22000) {
        throw 'Este instalador requer Windows 11 x64.'
    }

    if (Test-ApplicationHealth) {
        Open-Application
        exit 0
    }

    $installed = $null -ne (Get-Service -Name $serviceName -ErrorAction SilentlyContinue)
    if ($installed -and (Try-StartInstalledService)) {
        Open-Application
        exit 0
    }

    $package = Find-InstallerPackage
    Assert-InstallerSignature $package.FullName
    Assert-InstallerIdentity $package.FullName
    $installerExitCode = Invoke-Installer -MsiPath $package.FullName -Repair $installed

    if (-not (Wait-ApplicationHealth 120)) {
        if ($installerExitCode -in @(1641, 3010)) {
            throw 'A instalação foi concluída, mas o Windows precisa ser reiniciado antes do primeiro uso.'
        }
        throw 'A instalação terminou, mas o serviço não ficou disponível. Consulte o log do instalador ou solicite suporte.'
    }

    Open-Application
    exit 0
} catch {
    Write-Host ''
    Write-Host 'ERRO:' $_.Exception.Message -ForegroundColor Red
    exit 1
}
