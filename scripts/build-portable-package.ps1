[CmdletBinding()]
param(
    [switch]$Development,
    [string]$ServerExecutable,
    [string]$ProductLicenseFile,
    [string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$utf8 = [Text.UTF8Encoding]::new($false)
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifestPath = Join-Path $repositoryRoot 'src-tauri\Cargo.toml'
$minimumSqlCipher = [version]'4.17.0'
$publisherToken = '__ODS_SIGNING_CERT_THUMBPRINT__'
$packageHashToken = '__ODS_PACKAGE_SHA256__'
$applicationIcon = Join-Path $repositoryRoot 'src-tauri\icons\icon.ico'
$validationLicense = Join-Path $repositoryRoot 'installer\validation-NOT-FOR-DISTRIBUTION.txt'

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or
    -not [Environment]::Is64BitOperatingSystem) {
    throw 'A geração do pacote requer Windows x64.'
}
if (-not $ServerExecutable) {
    $ServerExecutable = Join-Path $repositoryRoot 'src-tauri\target\release\offline-dental-system.exe'
}
$ServerExecutable = [IO.Path]::GetFullPath($ServerExecutable)
if (-not (Test-Path -LiteralPath $ServerExecutable -PathType Leaf)) {
    throw "Executável release ausente em '$ServerExecutable'."
}

$cargoManifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath
$versionMatch = [regex]::Match($cargoManifest, '(?m)^version\s*=\s*"(\d+\.\d+\.\d+)"\s*$')
if (-not $versionMatch.Success) { throw 'Não foi possível ler a versão do produto.' }
$version = $versionMatch.Groups[1].Value

$diagnosticStdout = Join-Path ([IO.Path]::GetTempPath()) ("ods-diagnostics-$([guid]::NewGuid().ToString('N')).out")
$diagnosticStderr = "$diagnosticStdout.err"
try {
    $diagnosticProcess = Start-Process -FilePath $ServerExecutable `
        -ArgumentList @('--security-diagnostics', '--json') `
        -Wait -PassThru -NoNewWindow `
        -RedirectStandardOutput $diagnosticStdout `
        -RedirectStandardError $diagnosticStderr
    $diagnosticOutput = if (Test-Path -LiteralPath $diagnosticStdout) {
        Get-Content -Raw -Encoding UTF8 -LiteralPath $diagnosticStdout
    } else { '' }
    if ($diagnosticProcess.ExitCode -ne 0 -or -not $diagnosticOutput) {
        throw 'O diagnóstico de segurança do servidor falhou.'
    }
    $diagnostics = $diagnosticOutput | ConvertFrom-Json
} finally {
    Remove-Item -LiteralPath $diagnosticStdout, $diagnosticStderr -Force -ErrorAction SilentlyContinue
}
$cipherVersion = [version]$diagnostics.sqlcipherVersion
if ($cipherVersion -lt $minimumSqlCipher) {
    throw "SQLCipher $minimumSqlCipher ou superior é obrigatório."
}
if (-not $Development -and
    (-not ($diagnostics.distributionReady -is [bool]) -or -not $diagnostics.distributionReady)) {
    throw 'O servidor não está liberado pelos gates de distribuição.'
}

$signerThumbprint = $null
if ($Development) {
    $ProductLicenseFile = $null
} else {
    if (-not $ProductLicenseFile) { $ProductLicenseFile = $env:ODS_PRODUCT_LICENSE_FILE }
    if (-not $ProductLicenseFile -or -not (Test-Path -LiteralPath $ProductLicenseFile -PathType Leaf)) {
        throw 'A licença definitiva do produto não foi informada.'
    }
    $ProductLicenseFile = [IO.Path]::GetFullPath($ProductLicenseFile)
    if ($ProductLicenseFile.Equals([IO.Path]::GetFullPath($validationLicense), [StringComparison]::OrdinalIgnoreCase) -or
        (Get-Content -Raw -Encoding UTF8 -LiteralPath $ProductLicenseFile) -match '(?i)NOT[ -]FOR[ -]DISTRIBUTION') {
        throw 'O marcador de validação não pode integrar uma distribuição.'
    }
    $signerThumbprint = (([string]$env:ODS_SIGNING_CERT_THUMBPRINT -replace '\s', '')).ToUpperInvariant()
    if ($signerThumbprint -notmatch '^[0-9A-F]{40}$') {
        throw 'ODS_SIGNING_CERT_THUMBPRINT deve conter o thumbprint SHA-1 do publisher.'
    }
    $signature = Get-AuthenticodeSignature -LiteralPath $ServerExecutable
    $actualThumbprint = if ($null -ne $signature.SignerCertificate) {
        (([string]$signature.SignerCertificate.Thumbprint -replace '\s', '')).ToUpperInvariant()
    } else { '' }
    if ($signature.Status -ne 'Valid' -or $actualThumbprint -ne $signerThumbprint -or
        $null -eq $signature.TimeStamperCertificate) {
        throw 'O servidor deve possuir assinatura Authenticode válida do publisher e timestamp.'
    }
}

if (-not $Development -and
    (-not (Test-Path -LiteralPath $ProductLicenseFile -PathType Leaf) -or
     (Get-Item -LiteralPath $ProductLicenseFile).Length -eq 0)) {
    throw 'O arquivo de licença está ausente ou vazio.'
}
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repositoryRoot 'artifacts\portable' }
$outputRoot = [IO.Path]::GetFullPath($OutputDirectory)
$packageName = "OfflineDentalSystem-$version-windows-x64"
$output = Join-Path $outputRoot $packageName
if (Test-Path -LiteralPath $output) { throw "O pacote '$output' já existe; sobrescrita recusada." }

$requiredOperations = @(
    'Abrir-Sistema.bat',
    'Desinstalar-Sistema.bat',
    'Desinstalar-Tudo.bat',
    'scripts\open-local.ps1',
    'scripts\uninstall-local.ps1',
    'scripts\lib\windows-service.psm1',
    'installer\configure-host-trust.ps1'
)
$requiredBootstrap = @(
    'Instalar-e-Iniciar.bat',
    'Abrir-Sistema.bat',
    'Desinstalar-Sistema.bat',
    'Desinstalar-Tudo.bat',
    'scripts\install-local.ps1',
    'scripts\open-local.ps1',
    'scripts\uninstall-local.ps1',
    'scripts\lib\windows-service.psm1'
)
foreach ($relative in @($requiredOperations + $requiredBootstrap)) {
    $path = Join-Path $repositoryRoot $relative
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Arquivo obrigatório ausente: '$relative'." }
}
if (-not (Test-Path -LiteralPath $applicationIcon -PathType Leaf)) { throw 'Ícone da aplicação ausente.' }

$temporary = Join-Path ([IO.Path]::GetTempPath()) ('ods-portable-' + [guid]::NewGuid().ToString('N'))
$stagingOutput = Join-Path $outputRoot (".$packageName-" + [guid]::NewGuid().ToString('N') + '.partial')
New-Item -ItemType Directory -Path $temporary | Out-Null
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
New-Item -ItemType Directory -Path $stagingOutput | Out-Null
try {
    $payloadApp = Join-Path $temporary 'payload\app'
    $payloadOperations = Join-Path $temporary 'payload\operations'
    New-Item -ItemType Directory -Force -Path $payloadApp, (Join-Path $payloadOperations 'scripts') | Out-Null
    Copy-Item -LiteralPath $ServerExecutable -Destination (Join-Path $payloadApp 'offline-dental-system.exe')
    Copy-Item -LiteralPath $applicationIcon -Destination (Join-Path $payloadApp 'offline-dental-system.ico')
    $payloadLicense = Join-Path $payloadApp 'LICENSE.txt'
    if ($Development) {
        [IO.File]::WriteAllText(
            $payloadLicense,
            "VALIDATION ONLY - NOT A PRODUCT LICENSE - NOT FOR DISTRIBUTION`r`n",
            [Text.Encoding]::ASCII
        )
    } else {
        Copy-Item -LiteralPath $ProductLicenseFile -Destination $payloadLicense
    }
    foreach ($relative in $requiredOperations) {
        $source = Join-Path $repositoryRoot $relative
        $targetRelative = if ($relative -eq 'installer\configure-host-trust.ps1') {
            'scripts\configure-host-trust.ps1'
        } else { $relative }
        $target = Join-Path $payloadOperations $targetRelative
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
        Copy-Item -LiteralPath $source -Destination $target
    }

    $files = @(Get-ChildItem -LiteralPath (Join-Path $temporary 'payload') -File -Recurse | Sort-Object FullName | ForEach-Object {
        [ordered]@{
            path = $_.FullName.Substring($temporary.Length).TrimStart('\').Replace('\', '/')
            size = $_.Length
            sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToUpperInvariant()
        }
    })
    $packageManifest = [ordered]@{
        schemaVersion = 1
        product = 'Offline Dental System'
        version = $version
        architecture = 'x64'
        distribution = -not $Development
        signerThumbprint = $signerThumbprint
        files = $files
    }
    [IO.File]::WriteAllText(
        (Join-Path $temporary 'manifest.json'),
        ($packageManifest | ConvertTo-Json -Depth 5),
        [Text.UTF8Encoding]::new($true)
    )

    $zip = Join-Path $stagingOutput "$packageName.zip"
    Compress-Archive -Path (Join-Path $temporary '*') -DestinationPath $zip -CompressionLevel Optimal
    $zipHash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToUpperInvariant()
    [IO.File]::WriteAllText((Join-Path $stagingOutput "$packageName.sha256"), "$zipHash  $packageName.zip`r`n", [Text.Encoding]::ASCII)

    foreach ($relative in $requiredBootstrap) {
        $source = Join-Path $repositoryRoot $relative
        $target = Join-Path $stagingOutput $relative
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
        if ($relative -eq 'scripts\install-local.ps1' -and -not $Development) {
            $template = Get-Content -Raw -Encoding UTF8 -LiteralPath $source
            if ([regex]::Matches($template, [regex]::Escape($publisherToken)).Count -ne 1 -or
                [regex]::Matches($template, [regex]::Escape($packageHashToken)).Count -ne 1) {
                throw 'O bootstrapper não contém exatamente os marcadores de publisher e pacote.'
            }
            $rendered = $template.Replace($publisherToken, $signerThumbprint).Replace($packageHashToken, $zipHash)
            [IO.File]::WriteAllText($target, $rendered, [Text.UTF8Encoding]::new($true))
        } else {
            Copy-Item -LiteralPath $source -Destination $target
        }
    }
    if ($Development) {
        [IO.File]::WriteAllText(
            (Join-Path $stagingOutput 'DEVELOPMENT-NOT-FOR-DISTRIBUTION.txt'),
            "PACOTE LOCAL DE DESENVOLVIMENTO. NAO DISTRIBUIR OU USAR COM DADOS REAIS.`r`n",
            [Text.Encoding]::ASCII
        )
    }

    Move-Item -LiteralPath $stagingOutput -Destination $output
    Write-Host "SUCESSO: pacote portátil criado em '$output'." -ForegroundColor Green
    if ($Development) { Write-Warning 'Pacote local de desenvolvimento: não distribuir para terceiros.' }
} finally {
    if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Recurse -Force }
    if (Test-Path -LiteralPath $stagingOutput) { Remove-Item -LiteralPath $stagingOutput -Recurse -Force }
}
