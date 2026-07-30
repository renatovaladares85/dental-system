[CmdletBinding()]
param(
    [switch]$ValidationOnly,
    [string]$ServerExecutable,
    [string]$ProductLicenseFile,
    [string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# Windows PowerShell 5.1 does not guarantee that this automatic variable exists
# in a fresh process before the first native command reports an exit code.
$global:LASTEXITCODE = 0

$installerRoot = $PSScriptRoot
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $installerRoot '..'))
$packageSource = Join-Path $installerRoot 'Package.wxs'
$configureServiceScript = Join-Path $installerRoot 'configure-service.ps1'
$configureScript = Join-Path $installerRoot 'configure-host-trust.ps1'
$launchScript = Join-Path $installerRoot 'launch-after-install.ps1'
$minimumSqlCipher = [version]'4.17.0'
$wixVersion = '4.0.6'

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or
    -not [Environment]::Is64BitOperatingSystem) {
    throw 'A validação/geração do MSI requer Windows x64.'
}
if (-not $ServerExecutable) {
    $ServerExecutable = Join-Path $repositoryRoot 'src-tauri\target\release\offline-dental-system.exe'
}
$ServerExecutable = [IO.Path]::GetFullPath($ServerExecutable)
if (-not (Test-Path -LiteralPath $ServerExecutable -PathType Leaf)) {
    throw "Executável release ausente em '$ServerExecutable'."
}

$cargoManifest = Get-Content -Raw (Join-Path $repositoryRoot 'src-tauri\Cargo.toml')
$versionMatch = [regex]::Match($cargoManifest, '(?m)^version\s*=\s*"(\d+\.\d+\.\d+)"\s*$')
if (-not $versionMatch.Success) { throw 'Não foi possível ler a versão do produto no Cargo.toml.' }
$productVersion = $versionMatch.Groups[1].Value

$diagnosticsStartInfo = [Diagnostics.ProcessStartInfo]::new()
$diagnosticsStartInfo.FileName = $ServerExecutable
$diagnosticsStartInfo.Arguments = '--security-diagnostics --json'
$diagnosticsStartInfo.UseShellExecute = $false
$diagnosticsStartInfo.CreateNoWindow = $true
$diagnosticsStartInfo.RedirectStandardOutput = $true
$diagnosticsStartInfo.RedirectStandardError = $true
$diagnosticsProcess = [Diagnostics.Process]::new()
$diagnosticsProcess.StartInfo = $diagnosticsStartInfo
try {
    if (-not $diagnosticsProcess.Start()) {
        throw 'Não foi possível iniciar o diagnóstico de segurança do executável.'
    }
    $diagnosticsOutput = $diagnosticsProcess.StandardOutput.ReadToEndAsync()
    $diagnosticsError = $diagnosticsProcess.StandardError.ReadToEndAsync()
    if (-not $diagnosticsProcess.WaitForExit(15000)) {
        $diagnosticsProcess.Kill()
        $diagnosticsProcess.WaitForExit()
        $null = $diagnosticsOutput.GetAwaiter().GetResult()
        $null = $diagnosticsError.GetAwaiter().GetResult()
        throw 'O diagnóstico de segurança excedeu o limite de 15 segundos e foi encerrado.'
    }
    $diagnosticsText = $diagnosticsOutput.GetAwaiter().GetResult().Trim()
    $null = $diagnosticsError.GetAwaiter().GetResult()
    $diagnosticsExit = $diagnosticsProcess.ExitCode
} finally {
    $diagnosticsProcess.Dispose()
}
if ($diagnosticsExit -ne 0 -or -not $diagnosticsText) {
    throw 'O executável não forneceu diagnóstico de segurança para o gate do instalador.'
}
$diagnostics = $diagnosticsText | ConvertFrom-Json
$cipherVersion = $null
if ($diagnostics.sqlcipherVersion) {
    $cipherVersion = [version]$diagnostics.sqlcipherVersion
}
$distributionReady = $diagnostics.distributionReady -is [bool] -and $diagnostics.distributionReady -eq $true
if (-not $ValidationOnly -and
    ($null -eq $cipherVersion -or $cipherVersion -lt $minimumSqlCipher -or -not $distributionReady)) {
    throw "Distribuição bloqueada: SQLCipher >= $minimumSqlCipher e distributionReady=true são obrigatórios."
}

$validationLicenseFile = [IO.Path]::GetFullPath((Join-Path $installerRoot 'validation-NOT-FOR-DISTRIBUTION.txt'))
if ($ValidationOnly) {
    $ProductLicenseFile = $validationLicenseFile
} elseif (-not $ProductLicenseFile) {
    $ProductLicenseFile = $env:ODS_PRODUCT_LICENSE_FILE
}
if (-not $ProductLicenseFile -or -not (Test-Path -LiteralPath $ProductLicenseFile -PathType Leaf)) {
    throw 'Arquivo de licença do produto ausente. Defina -ProductLicenseFile ou ODS_PRODUCT_LICENSE_FILE.'
}
$ProductLicenseFile = [IO.Path]::GetFullPath($ProductLicenseFile)
if ((Get-Item -LiteralPath $ProductLicenseFile).Length -eq 0) { throw 'O arquivo de licença está vazio.' }
if (-not $ValidationOnly -and
    ($ProductLicenseFile.Equals($validationLicenseFile, [StringComparison]::OrdinalIgnoreCase) -or
     (Get-Content -Raw -LiteralPath $ProductLicenseFile) -match '(?i)NOT[ -]FOR[ -]DISTRIBUTION')) {
    throw 'Distribuição bloqueada: o marcador de validação não é uma licença de produto.'
}

$wix = Get-Command 'wix.exe' -ErrorAction SilentlyContinue
if ($null -eq $wix -and $env:ODS_WIX_EXE) {
    $wixCandidate = [IO.Path]::GetFullPath($env:ODS_WIX_EXE)
    if (-not (Test-Path -LiteralPath $wixCandidate -PathType Leaf)) {
        throw "ODS_WIX_EXE não aponta para um arquivo existente: '$wixCandidate'."
    }
    $wix = Get-Command $wixCandidate -ErrorAction Stop
}
if ($null -eq $wix) {
    throw "WiX Toolset $wixVersion ausente. Instale-o globalmente ou defina ODS_WIX_EXE para uma cópia portátil fixada."
}
$detectedWix = (& $wix.Source '--version' | Select-Object -First 1).Trim()
if ($detectedWix -notmatch '^4\.0\.6(?:\+|$)') {
    throw "WiX $wixVersion é obrigatório; detectado: $detectedWix."
}

foreach ($extension in @('WixToolset.Firewall.wixext', 'WixToolset.Util.wixext')) {
    # `extension add` is idempotent and also handles an initially empty global
    # cache (for which WiX 4 returns a non-zero status from `extension list`).
    & $wix.Source 'extension' 'add' '-g' "$extension/$wixVersion"
    if ($LASTEXITCODE -ne 0) { throw "Falha ao instalar a extensão WiX $extension/$wixVersion." }
}

function Find-SignTool {
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    if (-not (Test-Path -LiteralPath $kits -PathType Container)) { return $null }
    return Get-ChildItem -LiteralPath $kits -Directory |
        Sort-Object Name -Descending |
        ForEach-Object { Join-Path $_.FullName 'x64\signtool.exe' } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
        Select-Object -First 1
}

function Get-ExpectedSigningThumbprint {
    $thumbprint = (([string]$env:ODS_SIGNING_CERT_THUMBPRINT -replace '\s', '')).ToUpperInvariant()
    if (-not $thumbprint -or $thumbprint -notmatch '^[0-9A-F]{40}$') {
        throw 'ODS_SIGNING_CERT_THUMBPRINT deve conter exatamente o thumbprint SHA-1 hexadecimal do certificado.'
    }
    return $thumbprint
}

function Assert-ExpectedSignature([string]$Path, [string]$ExpectedThumbprint) {
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    $actualThumbprint = if ($null -ne $signature.SignerCertificate) {
        (([string]$signature.SignerCertificate.Thumbprint -replace '\s', '')).ToUpperInvariant()
    } else {
        ''
    }
    if ($signature.Status -ne 'Valid' -or $actualThumbprint -ne $ExpectedThumbprint) {
        throw "A assinatura de '$Path' não pertence ao certificado configurado ou não é válida."
    }
    if ($null -eq $signature.TimeStamperCertificate) {
        throw "A assinatura de '$Path' não possui timestamp verificável."
    }
}

function Sign-And-Verify([string]$Path) {
    $thumbprint = Get-ExpectedSigningThumbprint
    $timestampUrl = $env:ODS_SIGNING_TIMESTAMP_URL
    if (-not $timestampUrl -or -not [Uri]::IsWellFormedUriString($timestampUrl, [UriKind]::Absolute)) {
        throw 'ODS_SIGNING_TIMESTAMP_URL deve conter uma URL HTTPS válida.'
    }
    if (-not $timestampUrl.StartsWith('https://', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Somente timestamp HTTPS é permitido.'
    }
    $signTool = Find-SignTool
    if (-not $signTool) { throw 'signtool.exe x64 não foi encontrado no Windows SDK.' }
    $arguments = @('sign', '/fd', 'SHA256', '/sha1', $thumbprint, '/tr', $timestampUrl, '/td', 'SHA256')
    if ($env:ODS_SIGNING_CERT_STORE -eq 'machine') { $arguments += '/sm' }
    $arguments += $Path
    & $signTool @arguments
    if ($LASTEXITCODE -ne 0) { throw "Falha ao assinar '$Path'." }
    & $signTool 'verify' '/pa' '/all' $Path
    if ($LASTEXITCODE -ne 0) { throw "Assinatura inválida em '$Path'." }
    Assert-ExpectedSignature $Path $thumbprint
}

if (-not $ValidationOnly) {
    $expectedThumbprint = Get-ExpectedSigningThumbprint
    $signature = Get-AuthenticodeSignature -LiteralPath $ServerExecutable
    if ($signature.Status -eq 'NotSigned') {
        Sign-And-Verify $ServerExecutable
    } elseif ($signature.Status -eq 'Valid') {
        Assert-ExpectedSignature $ServerExecutable $expectedThumbprint
    } else {
        throw 'O executável possui uma assinatura inválida ou não confiável; a distribuição foi bloqueada.'
    }
}

$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ("offline-dental-msi-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
$temporaryMsi = Join-Path $temporaryDirectory "OfflineDentalSystem-$productVersion-x64.msi"
try {
    & $wix.Source 'build' $packageSource `
        '-arch' 'x64' `
        '-ext' "WixToolset.Firewall.wixext/$wixVersion" `
        '-ext' "WixToolset.Util.wixext/$wixVersion" `
        '-d' "ProductVersion=$productVersion" `
        '-d' "ServerExecutable=$ServerExecutable" `
        '-d' "ProductLicenseFile=$ProductLicenseFile" `
        '-d' "ConfigureServiceScript=$configureServiceScript" `
        '-d' "ConfigureHostTrustScript=$configureScript" `
        '-d' "LaunchAfterInstallScript=$launchScript" `
        '-o' $temporaryMsi
    if ($LASTEXITCODE -ne 0) { throw 'Compilação WiX falhou.' }

    & $wix.Source 'msi' 'validate' $temporaryMsi
    if ($LASTEXITCODE -ne 0) { throw 'Validação MSI falhou.' }

    if ($ValidationOnly) {
        Write-Host 'SUCESSO: fonte WiX compilada/validada; MSI temporário será removido (não distribuível).' -ForegroundColor Green
        return
    }

    Sign-And-Verify $temporaryMsi
    if (-not $OutputDirectory) {
        $OutputDirectory = Join-Path $repositoryRoot 'artifacts\installer'
    }
    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
    $output = Join-Path ([IO.Path]::GetFullPath($OutputDirectory)) (Split-Path -Leaf $temporaryMsi)
    if (Test-Path -LiteralPath $output) {
        throw "O instalador '$output' já existe; sobrescrita foi recusada."
    }
    Move-Item -LiteralPath $temporaryMsi -Destination $output
    Write-Host "SUCESSO: MSI assinado e validado em '$output'." -ForegroundColor Green
} finally {
    if (Test-Path -LiteralPath $temporaryDirectory) {
        Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force
    }
}
