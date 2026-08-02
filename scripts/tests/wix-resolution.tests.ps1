[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
. (Join-Path $repositoryRoot 'scripts\tools\wix-tooling.ps1')

$testRoot = Join-Path $repositoryRoot ('.local-data\tests\wix-resolution-' + [guid]::NewGuid().ToString('N'))
$logRoot = Join-Path $testRoot 'logs'
$emptyToolsRoot = Join-Path $testRoot 'empty-tools'
$localToolsRoot = Join-Path $testRoot 'local-tools'
$fixtureToolsRoot = Join-Path $testRoot 'fixture-tools'
$fixtureExtensionRoot = Join-Path $fixtureToolsRoot 'wix-extensions'
$backupRoot = Join-Path $testRoot 'backups'
$originalOdsWix = $env:ODS_WIX_EXE
$originalPath = $env:Path

function Assert-ThrowsContaining {
    param(
        [Parameter(Mandatory)][scriptblock]$Action,
        [Parameter(Mandatory)][string]$Expected
    )

    $actualMessage = $null
    try { & $Action } catch { $actualMessage = $_.Exception.Message }
    if (-not $actualMessage) { throw "Era esperada uma falha contendo '$Expected'." }
    if (-not $actualMessage.Contains($Expected, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Falha inesperada. Esperado '$Expected'; recebido: $actualMessage"
    }
}

function New-ExtensionFixture {
    param(
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][ValidateSet('WixToolset.Firewall.wixext', 'WixToolset.Util.wixext')][string]$PackageId,
        [switch]$WithoutMarker,
        [switch]$DuplicateDll
    )

    $packageRoot = Join-Path $Root "$PackageId\4.0.6"
    $dllDirectory = Join-Path $packageRoot 'wixext4'
    [void](New-Item -ItemType Directory -Path $dllDirectory -Force)
    $dllName = "$PackageId.dll"
    [IO.File]::WriteAllText((Join-Path $dllDirectory $dllName), 'fixture', [Text.UTF8Encoding]::new($false))
    if ($DuplicateDll) {
        $duplicateDirectory = Join-Path $packageRoot 'duplicate'
        [void](New-Item -ItemType Directory -Path $duplicateDirectory -Force)
        [IO.File]::WriteAllText((Join-Path $duplicateDirectory $dllName), 'duplicate', [Text.UTF8Encoding]::new($false))
    }
    if (-not $WithoutMarker) {
        $marker = [ordered]@{
            PackageId = $PackageId
            Version = '4.0.6'
            DllRelativePath = "wixext4\$dllName"
            PackageSha256 = 'TEST'
        } | ConvertTo-Json
        [IO.File]::WriteAllText((Join-Path $packageRoot '.ods-extension.json'), $marker, [Text.UTF8Encoding]::new($false))
    }
}

function New-WixExecutableFixture {
    param([Parameter(Mandatory)][string]$Path)

    $directory = Split-Path -Parent $Path
    [void](New-Item -ItemType Directory -Path $directory -Force)
    $source = @'
using System;
using System.Text;

public static class OdsWixFixture
{
    public static int Main(string[] args)
    {
        Console.OutputEncoding = new UTF8Encoding(false);
        Console.WriteLine("4.0.6+fixture");
        return 0;
    }
}
'@
    $sourcePath = Join-Path $directory 'wix-fixture.cs'
    [IO.File]::WriteAllText($sourcePath, $source, [Text.UTF8Encoding]::new($false))
    $compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
    if (-not (Test-Path -LiteralPath $compiler -PathType Leaf)) {
        throw "Compilador C# do .NET Framework não encontrado: '$compiler'."
    }
    $compilerProcess = Start-Process `
        -FilePath $compiler `
        -ArgumentList @('/nologo', '/target:exe', "/out:$Path", $sourcePath) `
        -Wait `
        -PassThru `
        -NoNewWindow
    if ($compilerProcess.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Falha ao compilar executável WiX auxiliar; código $($compilerProcess.ExitCode)."
    }
}

try {
    [void](New-Item -ItemType Directory -Path $testRoot, $emptyToolsRoot, $localToolsRoot, $fixtureToolsRoot -Force)
    $localWix = [IO.Path]::GetFullPath((Join-Path $localToolsRoot 'wix\4.0.6\wix.exe'))
    New-WixExecutableFixture -Path $localWix
    if (-not (Test-OdsWixExecutable -Executable $localWix -WorkingDirectory $repositoryRoot -LogRoot $logRoot)) {
        throw "Fixture WiX local válida não encontrada em '$localWix'."
    }

    $env:ODS_WIX_EXE = 'C:\caminho-inválido\wix.exe'
    $explicit = Resolve-OdsWixExecutable `
        -WixExecutable $localWix `
        -ToolsRoot $emptyToolsRoot `
        -RepositoryRoot $repositoryRoot `
        -LogRoot $logRoot
    if (-not $explicit.Equals($localWix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Parâmetro explícito válido não teve prioridade.'
    }

    $env:ODS_WIX_EXE = $localWix
    $fromEnvironment = Resolve-OdsWixExecutable `
        -ToolsRoot $emptyToolsRoot `
        -RepositoryRoot $repositoryRoot `
        -LogRoot $logRoot
    if (-not $fromEnvironment.Equals($localWix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'ODS_WIX_EXE válido não foi aceito.'
    }

    $env:ODS_WIX_EXE = 'C:\caminho-inválido\wix.exe'
    $fromLocal = Resolve-OdsWixExecutable `
        -ToolsRoot $localToolsRoot `
        -RepositoryRoot $repositoryRoot `
        -LogRoot $logRoot
    if (-not $fromLocal.Equals($localWix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'ODS_WIX_EXE inválido bloqueou o fallback local.'
    }

    $env:ODS_WIX_EXE = $null
    $env:Path = (Split-Path -Parent $localWix) + [IO.Path]::PathSeparator + $originalPath
    $fromPath = Resolve-OdsWixExecutable `
        -ToolsRoot $emptyToolsRoot `
        -RepositoryRoot $repositoryRoot `
        -LogRoot $logRoot
    if (-not $fromPath.Equals($localWix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'WiX válido no PATH não foi encontrado.'
    }

    $node = (Get-Command 'node.exe' -ErrorAction Stop).Source
    Assert-ThrowsContaining -Expected 'WiX 4.0.6 válido' -Action {
        Resolve-OdsWixExecutable `
            -WixExecutable $node `
            -ToolsRoot $emptyToolsRoot `
            -RepositoryRoot $repositoryRoot `
            -LogRoot $logRoot
    }

    New-ExtensionFixture -Root $fixtureExtensionRoot -PackageId 'WixToolset.Firewall.wixext'
    New-ExtensionFixture -Root $fixtureExtensionRoot -PackageId 'WixToolset.Util.wixext'
    $relativeExtensionRoot = [IO.Path]::GetRelativePath($repositoryRoot, $fixtureExtensionRoot)
    $extensions = Resolve-OdsWixExtensions `
        -WixExtensionRoot $relativeExtensionRoot `
        -ToolsRoot $fixtureToolsRoot `
        -RepositoryRoot $repositoryRoot
    foreach ($extensionPath in $extensions.FirewallExtension, $extensions.UtilExtension) {
        if (-not [IO.Path]::IsPathRooted($extensionPath) -or -not (Test-Path -LiteralPath $extensionPath -PathType Leaf)) {
            throw "Caminho de extensão não foi normalizado: '$extensionPath'."
        }
    }

    $missingRoot = Join-Path $testRoot 'missing-firewall'
    New-ExtensionFixture -Root $missingRoot -PackageId 'WixToolset.Util.wixext'
    Assert-ThrowsContaining -Expected 'Firewall' -Action {
        Resolve-OdsWixExtensions -WixExtensionRoot $missingRoot -ToolsRoot $fixtureToolsRoot -RepositoryRoot $repositoryRoot
    }

    $missingUtilRoot = Join-Path $testRoot 'missing-util'
    New-ExtensionFixture -Root $missingUtilRoot -PackageId 'WixToolset.Firewall.wixext'
    Assert-ThrowsContaining -Expected 'Util' -Action {
        Resolve-OdsWixExtensions -WixExtensionRoot $missingUtilRoot -ToolsRoot $fixtureToolsRoot -RepositoryRoot $repositoryRoot
    }

    $partialRoot = Join-Path $testRoot 'partial'
    New-ExtensionFixture -Root $partialRoot -PackageId 'WixToolset.Firewall.wixext' -WithoutMarker
    Assert-ThrowsContaining -Expected 'completamente' -Action {
        Get-OdsWixExtensionMetadata -ExtensionRoot $partialRoot -PackageId 'WixToolset.Firewall.wixext'
    }

    $duplicateRoot = Join-Path $testRoot 'duplicate'
    New-ExtensionFixture -Root $duplicateRoot -PackageId 'WixToolset.Util.wixext' -DuplicateDll
    Assert-ThrowsContaining -Expected 'exatamente uma DLL' -Action {
        Get-OdsWixExtensionMetadata -ExtensionRoot $duplicateRoot -PackageId 'WixToolset.Util.wixext'
    }

    $first = Initialize-OdsWixTooling `
        -WixExecutable $localWix `
        -ToolsRoot $fixtureToolsRoot `
        -RepositoryRoot $repositoryRoot
    $backupCountBefore = @(Get-ChildItem -LiteralPath $backupRoot -Force -ErrorAction SilentlyContinue).Count
    $second = Initialize-OdsWixTooling `
        -WixExecutable $localWix `
        -ToolsRoot $fixtureToolsRoot `
        -RepositoryRoot $repositoryRoot
    $backupCountAfter = @(Get-ChildItem -LiteralPath $backupRoot -Force -ErrorAction SilentlyContinue).Count
    if ($first.WixExecutable -cne $second.WixExecutable -or
        $first.FirewallExtension -cne $second.FirewallExtension -or
        $first.UtilExtension -cne $second.UtilExtension -or
        $backupCountBefore -ne $backupCountAfter) {
        throw 'Instalação válida não foi reutilizada de forma idempotente.'
    }

    $incomplete = Join-Path $testRoot 'incomplete-tool'
    [void](New-Item -ItemType Directory -Path $incomplete)
    [IO.File]::WriteAllText((Join-Path $incomplete 'partial.txt'), 'recoverable', [Text.UTF8Encoding]::new($false))
    $backup = Move-OdsWixDirectoryToBackup `
        -Path $incomplete `
        -BackupRoot $backupRoot `
        -Nature 'wix-test'
    if (-not (Test-Path -LiteralPath (Join-Path $backup 'partial.txt') -PathType Leaf) -or
        (Test-Path -LiteralPath $incomplete)) {
        throw 'Diretório incompleto não foi preservado em backup recuperável.'
    }

    $firewallUri = Get-OdsNuGetPackageUri -PackageId 'WixToolset.Firewall.wixext'
    $utilUri = Get-OdsNuGetPackageUri -PackageId 'WixToolset.Util.wixext'
    foreach ($uri in $firewallUri, $utilUri) {
        if (-not $uri.StartsWith('https://api.nuget.org/v3-flatcontainer/', [StringComparison]::Ordinal)) {
            throw "URL de pacote não usa o endpoint oficial fixado: '$uri'."
        }
        if ($uri -notmatch '/4\.0\.6/') { throw "URL de pacote não fixa 4.0.6: '$uri'." }
    }

    if ($env:ODS_WIX_EXE -ne $null -or $env:Path -cne ((Split-Path -Parent $localWix) + [IO.Path]::PathSeparator + $originalPath)) {
        throw 'O helper alterou configuração de processo fora do controle do teste.'
    }
} finally {
    $env:ODS_WIX_EXE = $originalOdsWix
    $env:Path = $originalPath
    if (Test-Path -LiteralPath $testRoot) { Remove-Item -LiteralPath $testRoot -Recurse -Force }
}

Write-Host 'SUCESSO: resolução, extensões, idempotência e backups WiX validados sem rede.' -ForegroundColor Green
