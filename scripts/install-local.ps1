[CmdletBinding()]
param(
    [string]$PackagePath,
    [string]$PackageUri,
    [string]$ExpectedPackageSha256,
    [switch]$AllowDevelopmentPackage,
    [switch]$ValidatePackageOnly,
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

$productName = 'Offline Dental System'
$serviceName = 'OfflineDentalSystem'
$healthUrl = 'http://127.0.0.1:8742/api/v1/health'
$applicationUrl = 'http://127.0.0.1:8742'
$expectedPublisherThumbprint = '__ODS_SIGNING_CERT_THUMBPRINT__'
$expectedBundledPackageSha256 = '__ODS_PACKAGE_SHA256__'
$bootstrapRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$serviceModule = Join-Path $PSScriptRoot 'lib\windows-service.psm1'
if (-not (Test-Path -LiteralPath $serviceModule -PathType Leaf)) {
    throw "Módulo obrigatório de serviço Windows ausente: '$serviceModule'."
}
Import-Module -Name $serviceModule -Force
$installRoot = Join-Path $env:ProgramFiles 'Offline Dental System'
$dataRoot = Join-Path $env:ProgramData 'OfflineDentalSystem'
$registryPath = 'HKLM:\Software\RenatoValadares\OfflineDentalSystem'
$firewallHttps = 'Offline Dental System HTTPS (Private-Domain)'
$firewallMdns = 'Offline Dental System mDNS (Private-Domain)'
$maximumArchiveBytes = 256MB
$maximumExpandedBytes = 512MB
$maximumArchiveEntries = 100
$allFirewallRules = @(
    $firewallHttps,
    $firewallMdns,
    'Offline Dental System HTTPS (Private)',
    'Offline Dental System HTTPS (Domain)',
    'Offline Dental System mDNS (Private)',
    'Offline Dental System mDNS (Domain)'
)
$script:downloadedPackage = $null

if (-not $RunId) { $RunId = [guid]::NewGuid().ToString('N') }
if ($RunId -notmatch '^[0-9a-f]{32}$') { throw 'RunId de instalação inválido.' }
if (-not $LogPath) {
    $logDirectory = Join-Path $env:LOCALAPPDATA 'OfflineDentalSystem\Logs\installation'
    New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
    $LogPath = Join-Path $logDirectory ("install-$([DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'))-$RunId.jsonl")
}
$LogPath = [IO.Path]::GetFullPath($LogPath)
$allowedLogRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'OfflineDentalSystem\Logs')).TrimEnd('\') + '\'
if (-not $LogPath.StartsWith($allowedLogRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'O log da instalação deve permanecer no diretório local controlado.'
}
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $LogPath) | Out-Null

function Write-InstallEvent {
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
    $line = $record | ConvertTo-Json -Compress -Depth 5
    [IO.File]::AppendAllText($LogPath, $line + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
}

function Write-Step([string]$Message) {
    Write-Host "`n==> $Message" -ForegroundColor Cyan
    Write-InstallEvent -Level INFO -Event 'INSTALL_STEP' -Message $Message
}

function Test-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Assert-NotReparsePoint([string]$Path, [string]$Description) {
    if (Test-Path -LiteralPath $Path) {
        $item = Get-Item -LiteralPath $Path -Force
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw "$Description não pode ser um link, junction ou reparse point."
        }
    }
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

function Resolve-Package {
    if ($PackagePath) {
        $resolved = [IO.Path]::GetFullPath($PackagePath)
        if (-not (Test-Path -LiteralPath $resolved -PathType Leaf)) {
            throw "O pacote informado não existe: '$resolved'."
        }
        return $resolved
    }

    $localPackages = @(Get-ChildItem -LiteralPath $bootstrapRoot -Filter 'OfflineDentalSystem-*-windows-x64.zip' -File -ErrorAction SilentlyContinue)
    $sourceArtifacts = Join-Path $bootstrapRoot 'artifacts\portable'
    if ($localPackages.Count -eq 0 -and (Test-Path -LiteralPath $sourceArtifacts -PathType Container)) {
        $localPackages = @(Get-ChildItem -LiteralPath $sourceArtifacts -Filter 'OfflineDentalSystem-*-windows-x64.zip' -File -Recurse)
    }
    if ($localPackages.Count -eq 1) { return $localPackages[0].FullName }
    if ($localPackages.Count -gt 1) {
        throw 'Mais de um pacote local foi encontrado. Mantenha somente a versão que será instalada.'
    }

    $channelPath = Join-Path $bootstrapRoot 'canal-instalacao.json'
    if (-not $PackageUri -and (Test-Path -LiteralPath $channelPath -PathType Leaf)) {
        $channel = Get-Content -Raw -Encoding UTF8 -LiteralPath $channelPath | ConvertFrom-Json
        if ($channel.schemaVersion -ne 1) { throw 'A versão de canal-instalacao.json não é suportada.' }
        $PackageUri = [string]$channel.packageUri
        $ExpectedPackageSha256 = [string]$channel.sha256
    }
    if (-not $PackageUri) {
        throw 'O pacote pré-compilado não foi encontrado. Mantenha o ZIP junto do BAT ou configure canal-instalacao.json.'
    }
    if (-not [Uri]::IsWellFormedUriString($PackageUri, [UriKind]::Absolute) -or
        -not $PackageUri.StartsWith('https://', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'O endereço do pacote deve ser uma URL HTTPS absoluta.'
    }
    $expected = (($ExpectedPackageSha256 -replace '\s', '')).ToUpperInvariant()
    if ($expected -notmatch '^[0-9A-F]{64}$') {
        throw 'O canal de instalação deve informar o SHA-256 esperado do pacote.'
    }

    Write-Step 'Baixando o pacote pré-compilado'
    $download = Join-Path ([IO.Path]::GetTempPath()) ("OfflineDentalSystem-" + [guid]::NewGuid().ToString('N') + '.zip')
    Invoke-WebRequest -Uri $PackageUri -UseBasicParsing -OutFile $download -TimeoutSec 120
    if ((Get-Item -LiteralPath $download).Length -gt $maximumArchiveBytes) {
        Remove-Item -LiteralPath $download -Force -ErrorAction SilentlyContinue
        throw 'O pacote baixado excede o limite de tamanho permitido.'
    }
    $actual = (Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash.ToUpperInvariant()
    if ($actual -ne $expected) {
        Remove-Item -LiteralPath $download -Force -ErrorAction SilentlyContinue
        throw 'O pacote baixado não corresponde ao SHA-256 publicado.'
    }
    $script:downloadedPackage = $download
    return $download
}

function Expand-SafeArchive([string]$Archive, [string]$Destination) {
    if ((Get-Item -LiteralPath $Archive).Length -gt $maximumArchiveBytes) {
        throw 'O pacote excede o limite de tamanho permitido.'
    }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $destinationRoot = [IO.Path]::GetFullPath($Destination).TrimEnd('\') + '\'
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        if ($zip.Entries.Count -gt $maximumArchiveEntries) {
            throw 'O pacote contém arquivos demais.'
        }
        $expandedBytes = [int64]0
        foreach ($entry in $zip.Entries) {
            $expandedBytes += [int64]$entry.Length
            if ($expandedBytes -gt $maximumExpandedBytes) {
                throw 'O conteúdo expandido excede o limite de tamanho permitido.'
            }
        }
        foreach ($entry in $zip.Entries) {
            $relative = $entry.FullName.Replace('/', '\')
            if (-not $relative -or [IO.Path]::IsPathRooted($relative) -or
                $relative.Split('\') -contains '..') {
                throw 'O pacote contém um caminho inseguro.'
            }
            $target = [IO.Path]::GetFullPath((Join-Path $Destination $relative))
            if (-not $target.StartsWith($destinationRoot, [StringComparison]::OrdinalIgnoreCase)) {
                throw 'O pacote tentou gravar fora do diretório temporário.'
            }
            if ($entry.FullName.EndsWith('/')) {
                New-Item -ItemType Directory -Force -Path $target | Out-Null
                continue
            }
            $parent = Split-Path -Parent $target
            New-Item -ItemType Directory -Force -Path $parent | Out-Null
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $target, $false)
        }
    } finally {
        $zip.Dispose()
    }
}

function Assert-Package([string]$Archive, [string]$ExtractionRoot) {
    $actualArchiveHash = (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToUpperInvariant()
    $pinnedArchiveHash = (($expectedBundledPackageSha256 -replace '\s', '')).ToUpperInvariant()
    if ($pinnedArchiveHash -match '^[0-9A-F]{64}$') {
        if ($actualArchiveHash -ne $pinnedArchiveHash) {
            throw 'O ZIP não corresponde ao pacote fixado pelo bootstrapper.'
        }
    } elseif (-not $AllowDevelopmentPackage) {
        throw 'O bootstrapper não contém o SHA-256 de uma distribuição aprovada.'
    }
    Expand-SafeArchive $Archive $ExtractionRoot
    $manifestPath = Join-Path $ExtractionRoot 'manifest.json'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw 'Manifesto ausente no pacote.' }
    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
    if ($manifest.schemaVersion -ne 1 -or $manifest.product -ne $productName -or
        $manifest.architecture -ne 'x64' -or [string]$manifest.version -notmatch '^\d+\.\d+\.\d+$') {
        throw 'O manifesto do pacote é incompatível.'
    }
    $files = @($manifest.files)
    if ($files.Count -eq 0) { throw 'O manifesto não contém arquivos.' }
    $declared = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($file in $files) {
        $relative = ([string]$file.path).Replace('/', '\')
        if ($relative -notmatch '^payload\\[A-Za-z0-9][A-Za-z0-9._\\ -]*$' -or $relative.Split('\') -contains '..') {
            throw 'O manifesto contém um caminho de arquivo inválido.'
        }
        if (-not $declared.Add($relative)) { throw 'O manifesto contém arquivos duplicados.' }
        $full = [IO.Path]::GetFullPath((Join-Path $ExtractionRoot $relative))
        if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { throw "Arquivo ausente no pacote: '$relative'." }
        if ((Get-Item -LiteralPath $full).Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw 'O pacote contém um reparse point não permitido.'
        }
        if ((Get-Item -LiteralPath $full).Length -ne [int64]$file.size) {
            throw "Tamanho divergente no pacote: '$relative'."
        }
        $actualHash = (Get-FileHash -LiteralPath $full -Algorithm SHA256).Hash.ToUpperInvariant()
        if ($actualHash -ne ([string]$file.sha256).ToUpperInvariant()) {
            throw "SHA-256 divergente no pacote: '$relative'."
        }
    }
    $actualFiles = @(Get-ChildItem -LiteralPath (Join-Path $ExtractionRoot 'payload') -File -Recurse | ForEach-Object {
        $_.FullName.Substring($ExtractionRoot.Length).TrimStart('\')
    })
    if ($actualFiles.Count -ne $declared.Count -or @($actualFiles | Where-Object { -not $declared.Contains($_) }).Count -gt 0) {
        throw 'O pacote contém arquivos não declarados no manifesto.'
    }

    $server = Join-Path $ExtractionRoot 'payload\app\offline-dental-system.exe'
    if (-not (Test-Path -LiteralPath $server -PathType Leaf)) { throw 'Executável do servidor ausente.' }
    $isDistribution = $manifest.distribution -is [bool] -and $manifest.distribution
    if (-not $isDistribution) {
        if (-not $AllowDevelopmentPackage) {
            throw 'Pacote de desenvolvimento recusado. Use somente uma distribuição assinada.'
        }
        Write-Warning 'Pacote de desenvolvimento sem assinatura: permitido apenas dentro do repositório local.'
    } else {
        $expected = (($expectedPublisherThumbprint -replace '\s', '')).ToUpperInvariant()
        if ($expected -notmatch '^[0-9A-F]{40}$' -or
            $expected -ne (([string]$manifest.signerThumbprint -replace '\s', '')).ToUpperInvariant()) {
            throw 'O publisher esperado pelo bootstrapper não corresponde ao manifesto.'
        }
        $signature = Get-AuthenticodeSignature -LiteralPath $server
        $actual = if ($null -ne $signature.SignerCertificate) {
            (([string]$signature.SignerCertificate.Thumbprint -replace '\s', '')).ToUpperInvariant()
        } else { '' }
        if ($signature.Status -ne 'Valid' -or $actual -ne $expected -or $null -eq $signature.TimeStamperCertificate) {
            throw 'A assinatura ou o timestamp do servidor é inválido.'
        }
    }
    return [pscustomobject]@{ Manifest = $manifest; Server = $server }
}

function Invoke-Sc(
    [string[]]$Arguments,
    [int[]]$AcceptedCodes = @(0),
    [hashtable]$LogData = @{}
) {
    $sc = Join-Path $env:SystemRoot 'System32\sc.exe'
    $metadata = @{
        operation = if ($Arguments.Count -gt 0) { $Arguments[0] } else { 'unknown' }
        serviceName = $serviceName
    }
    foreach ($key in $LogData.Keys) { $metadata[$key] = $LogData[$key] }
    $eventWriter = {
        param($Level, $Event, $Message, $Data)
        Write-InstallEvent -Level $Level -Event $Event -Message $Message -Data $Data
    }
    Invoke-OdsNativeProcess -FilePath $sc -Arguments $Arguments -AcceptedExitCodes $AcceptedCodes `
        -EventWriter $eventWriter -EventData $metadata | Out-Null
}

function Set-DataRootAcl([string]$Path) {
    New-Item -ItemType Directory -Force -Path $Path | Out-Null
    $serviceSid = ([Security.Principal.NTAccount]::new('NT SERVICE', $serviceName)).Translate([Security.Principal.SecurityIdentifier])
    $acl = [Security.AccessControl.DirectorySecurity]::new()
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($sidValue in @('S-1-5-18', 'S-1-5-32-544', $serviceSid.Value)) {
        $sid = [Security.Principal.SecurityIdentifier]::new($sidValue)
        $rule = [Security.AccessControl.FileSystemAccessRule]::new(
            $sid,
            [Security.AccessControl.FileSystemRights]::FullControl,
            [Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit',
            [Security.AccessControl.PropagationFlags]::None,
            [Security.AccessControl.AccessControlType]::Allow
        )
        $acl.AddAccessRule($rule)
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}

function Set-Firewall([string]$Executable) {
    foreach ($rule in $allFirewallRules) {
        Remove-NetFirewallRule -DisplayName $rule -ErrorAction SilentlyContinue
    }
    New-NetFirewallRule -DisplayName $firewallHttps -Direction Inbound -Action Allow -Enabled True `
        -Profile Private, Domain -Program $Executable -Protocol TCP -LocalPort 8743 -RemoteAddress LocalSubnet | Out-Null
    New-NetFirewallRule -DisplayName $firewallMdns -Direction Inbound -Action Allow -Enabled True `
        -Profile Private, Domain -Program $Executable -Protocol UDP -LocalPort 5353 -RemoteAddress LocalSubnet | Out-Null
}

function New-Shortcut([string]$Path, [string]$Target, [string]$WorkingDirectory, [string]$Icon) {
    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $null
    try {
        $shortcut = $shell.CreateShortcut($Path)
        $shortcut.TargetPath = $Target
        $shortcut.WorkingDirectory = $WorkingDirectory
        $shortcut.IconLocation = "$Icon,0"
        $shortcut.Save()
    } finally {
        if ($null -ne $shortcut) { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shortcut) }
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)
    }
}

function Install-Shortcuts([string]$Root, [string]$Icon) {
    $openBatch = Join-Path $Root 'Abrir-Sistema.bat'
    $uninstallBatch = Join-Path $Root 'Desinstalar-Sistema.bat'
    $desktop = Join-Path $env:PUBLIC 'Desktop\Offline Dental System.lnk'
    $startMenu = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\Offline Dental System'
    New-Shortcut $desktop $openBatch $env:TEMP $Icon
    New-Shortcut (Join-Path $startMenu 'Offline Dental System.lnk') $openBatch $env:TEMP $Icon
    New-Shortcut (Join-Path $startMenu 'Desinstalar Offline Dental System.lnk') $uninstallBatch $env:TEMP $Icon
}

try {
    Write-InstallEvent -Level INFO -Event 'INSTALL_STARTED' -Message 'Instalação iniciada.' -Data @{
        validateOnly = [bool]$ValidatePackageOnly
        developmentPackageAllowed = [bool]$AllowDevelopmentPackage
    }
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or
        -not [Environment]::Is64BitOperatingSystem -or
        [Environment]::OSVersion.Version.Build -lt 22000) {
        throw 'Este instalador requer Windows 11 x64.'
    }
    if (-not $ValidatePackageOnly -and (Test-Health)) {
        Write-InstallEvent -Level INFO -Event 'INSTALL_ALREADY_HEALTHY' -Message 'Instalação existente respondeu ao health check.'
        Start-Process $applicationUrl
        exit 0
    }

    $resolvedPackage = Resolve-Package
    Write-InstallEvent -Level INFO -Event 'PACKAGE_RESOLVED' -Message 'Pacote localizado.' -Data @{
        fileName = [IO.Path]::GetFileName($resolvedPackage)
        sha256 = (Get-FileHash -LiteralPath $resolvedPackage -Algorithm SHA256).Hash.ToUpperInvariant()
    }
    $extractRoot = Join-Path ([IO.Path]::GetTempPath()) ('ods-package-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $extractRoot | Out-Null
    try {
        Write-Step 'Validando integridade e identidade do pacote'
        $package = Assert-Package $resolvedPackage $extractRoot
        Write-InstallEvent -Level INFO -Event 'PACKAGE_VALIDATED' -Message 'Pacote validado.' -Data @{
            version = [string]$package.Manifest.version
            distribution = [bool]$package.Manifest.distribution
        }
        if ($ValidatePackageOnly) {
            Write-InstallEvent -Level INFO -Event 'PACKAGE_VALIDATION_COMPLETED' -Message 'Validação concluída sem alterar o computador.'
            Write-Host "SUCESSO: pacote $($package.Manifest.version) validado sem alterar o computador." -ForegroundColor Green
            Write-Host "Log: $LogPath"
            exit 0
        }

        if (-not $Elevated -and -not (Test-Administrator)) {
            Write-InstallEvent -Level INFO -Event 'ELEVATION_REQUESTED' -Message 'Solicitando privilégios administrativos.'
            $arguments = @(
                '-NoLogo', '-NoProfile', '-ExecutionPolicy', 'Bypass',
                '-File', $PSCommandPath,
                '-PackagePath', $resolvedPackage,
                '-Elevated',
                '-RunId', $RunId,
                '-LogPath', $LogPath
            )
            if ($AllowDevelopmentPackage) { $arguments += '-AllowDevelopmentPackage' }
            try {
                $process = Start-OdsProcess -FilePath 'powershell.exe' -Arguments $arguments -Verb RunAs -Wait -PassThru
            } catch [ComponentModel.Win32Exception] {
                if ($_.Exception.NativeErrorCode -eq 1223) { throw 'Instalação cancelada na confirmação de segurança do Windows.' }
                throw
            }
            if ($process.ExitCode -ne 0) {
                $lastError = Get-Content -Encoding UTF8 -LiteralPath $LogPath -ErrorAction SilentlyContinue |
                    ForEach-Object { try { $_ | ConvertFrom-Json } catch { $null } } |
                    Where-Object { $_.level -eq 'ERROR' -and $_.event -ne 'ELEVATED_PROCESS_COMPLETED' } |
                    Select-Object -Last 1
                $detail = if ($lastError -and $lastError.message) { [string]$lastError.message } else { 'Falha administrativa sem detalhe adicional.' }
                Write-InstallEvent -Level ERROR -Event 'ELEVATED_PROCESS_COMPLETED' `
                    -Message 'O processo elevado falhou.' `
                    -Data @{ exitCode = $process.ExitCode; childError = $detail }
                throw "$detail Consulte o log '$LogPath'."
            }
            Write-InstallEvent -Level INFO -Event 'ELEVATED_PROCESS_COMPLETED' `
                -Message 'Processo elevado concluído com sucesso.' `
                -Data @{ exitCode = $process.ExitCode }
            Write-Host "Log: $LogPath"
            exit 0
        }
        if (-not (Test-Administrator)) { throw 'A instalação requer privilégios de administrador.' }

        Assert-NotReparsePoint $installRoot 'O diretório da aplicação'
        Assert-NotReparsePoint $dataRoot 'O diretório de dados'
        $installRootExistedBefore = Test-Path -LiteralPath $installRoot
        $desktopShortcut = Join-Path $env:PUBLIC 'Desktop\Offline Dental System.lnk'
        $startMenuDirectory = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\Offline Dental System'
        $shortcutsExistedBefore = (Test-Path -LiteralPath $desktopShortcut) -or (Test-Path -LiteralPath $startMenuDirectory)

        Write-Step "Instalando $productName $($package.Manifest.version)"
        $version = [string]$package.Manifest.version
        $versionsRoot = Join-Path $installRoot 'versions'
        $versionRoot = Join-Path $versionsRoot $version
        $server = Join-Path $versionRoot 'offline-dental-system.exe'
        $versionStaging = Join-Path $versionsRoot (".$version-" + [guid]::NewGuid().ToString('N') + '.partial')
        $serviceSnapshot = Get-OdsWindowsServiceSnapshot -ServiceName $serviceName
        Assert-OdsWindowsServiceCompatible -Snapshot $serviceSnapshot -ServiceName $serviceName -InstallRoot $installRoot
        $serviceCreatedThisRun = $false
        $serviceChangedThisRun = $false
        $firewallConfiguredThisRun = $false
        $shortcutsInstalledThisRun = $false
        $serviceInvoker = {
            param($Arguments, $AcceptedCodes, $Metadata)
            Invoke-Sc -Arguments $Arguments -AcceptedCodes $AcceptedCodes -LogData $Metadata
        }
        $snapshotProvider = { Get-OdsWindowsServiceSnapshot -ServiceName $serviceName }
        Assert-NotReparsePoint $versionsRoot 'O diretório de versões'
        New-Item -ItemType Directory -Force -Path $versionsRoot | Out-Null
        New-Item -ItemType Directory -Path $versionStaging | Out-Null
        Copy-Item -Path (Join-Path $extractRoot 'payload\app\*') -Destination $versionStaging -Recurse
        Write-InstallEvent -Level INFO -Event 'PAYLOAD_STAGED' -Message 'Payload preparado em diretório temporário.' -Data @{ version = $version }

        $rollback = $null
        try {
            if (Test-Path -LiteralPath $versionRoot) {
                $rollback = "$versionRoot.rollback-$([guid]::NewGuid().ToString('N'))"
                $existingService = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
                if ($null -ne $existingService -and $existingService.Status -ne 'Stopped') {
                    $serviceChangedThisRun = $true
                    Invoke-Sc @('stop', $serviceName) @(0, 1062)
                    $existingService.WaitForStatus([ServiceProcess.ServiceControllerStatus]::Stopped, [TimeSpan]::FromSeconds(30))
                }
                Move-Item -LiteralPath $versionRoot -Destination $rollback
            }
            Move-Item -LiteralPath $versionStaging -Destination $versionRoot
            Write-InstallEvent -Level INFO -Event 'PAYLOAD_PUBLISHED' -Message 'Versão publicada no diretório da aplicação.' -Data @{ version = $version }

            Copy-Item -Path (Join-Path $extractRoot 'payload\operations\*') -Destination $installRoot -Recurse -Force
            $icon = Join-Path $versionRoot 'offline-dental-system.ico'
            Write-Step 'Registrando e restringindo o serviço Windows'
            $verifiedService = Set-OdsWindowsService -ServiceName $serviceName -ProductName $productName `
                -Executable $server -InstallRoot $installRoot `
                -CreatedThisRun ([ref]$serviceCreatedThisRun) -ChangedThisRun ([ref]$serviceChangedThisRun) `
                -NativeInvoker $serviceInvoker -SnapshotProvider $snapshotProvider
            Write-InstallEvent -Level INFO -Event 'SERVICE_CONFIGURATION_VERIFIED' `
                -Message 'Configuração persistida do serviço validada.' -Data @{
                    serviceName = $serviceName
                    imagePath = [string]$verifiedService.ImagePath
                    account = [string]$verifiedService.ObjectName
                    start = [int]$verifiedService.Start
                    delayedAutoStart = [int]$verifiedService.DelayedAutoStart
                    serviceSidType = [int]$verifiedService.ServiceSidType
                }
            Write-Step 'Aplicando proteção ao diretório de dados'
            Set-DataRootAcl $dataRoot

            Write-Step 'Iniciando o servidor e aguardando o health check'
            Invoke-Sc @('start', $serviceName)
            if (-not (Wait-Health 90)) { throw 'O serviço foi instalado, mas não respondeu ao health check.' }

            Write-Step 'Criando atalhos e regras de rede locais'
            Install-Shortcuts $installRoot $icon
            $shortcutsInstalledThisRun = $true
            Set-Firewall $server
            $firewallConfiguredThisRun = $true
            Write-Step 'Confiando na autoridade certificadora local'
            $trustScript = Join-Path $installRoot 'scripts\configure-host-trust.ps1'
            & $trustScript -ProductRoot $dataRoot

            New-Item -Path $registryPath -Force | Out-Null
            New-ItemProperty -Path $registryPath -Name InstallRoot -Value $installRoot -PropertyType String -Force | Out-Null
            New-ItemProperty -Path $registryPath -Name DataRoot -Value $dataRoot -PropertyType String -Force | Out-Null
            New-ItemProperty -Path $registryPath -Name Version -Value $version -PropertyType String -Force | Out-Null
            New-ItemProperty -Path $registryPath -Name ServerExecutable -Value $server -PropertyType String -Force | Out-Null

            if ($rollback -and (Test-Path -LiteralPath $rollback)) {
                Remove-Item -LiteralPath $rollback -Recurse -Force
            }
        } catch {
            $operationError = $_
            Write-InstallEvent -Level ERROR -Event 'INSTALL_OPERATION_FAILED' -Message $operationError.Exception.Message -Data @{
                exceptionType = $operationError.Exception.GetType().FullName
                line = $operationError.InvocationInfo.ScriptLineNumber
            }
            try {
                if ($serviceCreatedThisRun) {
                    Undo-OdsWindowsServiceChange -ServiceName $serviceName `
                        -CreatedThisRun $true -ChangedThisRun $serviceChangedThisRun `
                        -PreviousSnapshot $serviceSnapshot -NativeInvoker $serviceInvoker `
                        -SnapshotProvider $snapshotProvider
                }
                if (Test-Path -LiteralPath $versionRoot) { Remove-Item -LiteralPath $versionRoot -Recurse -Force }
                if ($rollback -and (Test-Path -LiteralPath $rollback)) { Move-Item -LiteralPath $rollback -Destination $versionRoot }
                if (-not $serviceCreatedThisRun) {
                    Undo-OdsWindowsServiceChange -ServiceName $serviceName `
                        -CreatedThisRun $false -ChangedThisRun $serviceChangedThisRun `
                        -PreviousSnapshot $serviceSnapshot -NativeInvoker $serviceInvoker `
                        -SnapshotProvider $snapshotProvider
                }
                if ($firewallConfiguredThisRun -and -not $serviceSnapshot.Exists) {
                    foreach ($rule in $allFirewallRules) {
                        Remove-NetFirewallRule -DisplayName $rule -ErrorAction SilentlyContinue
                    }
                } elseif ($firewallConfiguredThisRun -and $serviceSnapshot.Exists) {
                    $previousMatch = [regex]::Match([string]$serviceSnapshot.ImagePath, '^"([^"]+)" --service$')
                    if ($previousMatch.Success -and (Test-Path -LiteralPath $previousMatch.Groups[1].Value -PathType Leaf)) {
                        try { Set-Firewall $previousMatch.Groups[1].Value } catch { Write-Warning 'Não foi possível restaurar automaticamente as regras de firewall anteriores.' }
                    }
                }
                if ($shortcutsInstalledThisRun -and -not $shortcutsExistedBefore) {
                    Remove-Item -LiteralPath $desktopShortcut -Force -ErrorAction SilentlyContinue
                    Remove-Item -LiteralPath $startMenuDirectory -Recurse -Force -ErrorAction SilentlyContinue
                }
                if (-not $installRootExistedBefore -and (Test-Path -LiteralPath $installRoot)) {
                    Remove-Item -LiteralPath $installRoot -Recurse -Force
                }
                Write-InstallEvent -Level WARN -Event 'INSTALL_ROLLBACK_COMPLETED' -Message 'Rollback concluído após falha.'
            } catch {
                Write-InstallEvent -Level ERROR -Event 'INSTALL_ROLLBACK_FAILED' -Message $_.Exception.Message -Data @{
                    exceptionType = $_.Exception.GetType().FullName
                    line = $_.InvocationInfo.ScriptLineNumber
                }
            }
            throw $operationError
        }

        Write-InstallEvent -Level INFO -Event 'INSTALL_COMPLETED' -Message 'Instalação concluída e health check aprovado.' -Data @{ version = $version }
        Write-Host "`nSUCESSO: sistema instalado e disponível em $applicationUrl" -ForegroundColor Green
        Write-Host "Log: $LogPath"
        Start-Process $applicationUrl
        exit 0
    } finally {
        if (Test-Path -LiteralPath $extractRoot) { Remove-Item -LiteralPath $extractRoot -Recurse -Force }
    }
} catch {
    try {
        Write-InstallEvent -Level ERROR -Event 'INSTALL_FAILED' -Message $_.Exception.Message -Data @{
            exceptionType = $_.Exception.GetType().FullName
            line = $_.InvocationInfo.ScriptLineNumber
        }
    } catch {
        # A mensagem original continua prioritária mesmo se o disco de log falhar.
    }
    Write-Host ''
    Write-Host 'ERRO:' $_.Exception.Message -ForegroundColor Red
    Write-Host "Log: $LogPath" -ForegroundColor Yellow
    exit 1
} finally {
    if ($script:downloadedPackage -and (Test-Path -LiteralPath $script:downloadedPackage)) {
        Remove-Item -LiteralPath $script:downloadedPackage -Force -ErrorAction SilentlyContinue
    }
}
