$script:OdsRequiredWixVersion = '4.0.6'
$script:OdsFirewallExtensionId = 'WixToolset.Firewall.wixext'
$script:OdsUtilExtensionId = 'WixToolset.Util.wixext'

function Resolve-OdsWixAbsolutePath {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$BasePath
    )

    if ([IO.Path]::IsPathRooted($Path)) { return [IO.Path]::GetFullPath($Path) }
    return [IO.Path]::GetFullPath((Join-Path $BasePath $Path))
}

function ConvertTo-OdsWixCommandLine {
    param([Parameter(Mandatory)][string[]]$Arguments)

    return ($Arguments | ForEach-Object {
        $value = [string]$_
        if ($value.Length -eq 0) { return '""' }
        if ($value -notmatch '[\s"]') { return $value }
        $escaped = [regex]::Replace($value, '(\\*)"', '$1$1\\"')
        $escaped = [regex]::Replace($escaped, '(\\+)$', '$1$1')
        return '"' + $escaped + '"'
    }) -join ' '
}

function Invoke-OdsWixProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Stage,
        [Parameter(Mandatory)][string]$Executable,
        [Parameter(Mandatory)][string[]]$Arguments,
        [Parameter(Mandatory)][string]$WorkingDirectory,
        [Parameter(Mandatory)][string]$LogRoot,
        [string]$ExpectedVersion = $script:OdsRequiredWixVersion,
        [switch]$PreserveSuccessLogs
    )

    $resolvedExecutable = [IO.Path]::GetFullPath($Executable)
    $resolvedWorkingDirectory = [IO.Path]::GetFullPath($WorkingDirectory)
    $resolvedLogRoot = [IO.Path]::GetFullPath($LogRoot)
    [void](New-Item -ItemType Directory -Path $resolvedLogRoot -Force)

    $safeStage = [regex]::Replace($Stage, '[^A-Za-z0-9._-]', '-')
    $identifier = [guid]::NewGuid().ToString('N')
    $timestamp = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')
    $stdoutLog = Join-Path $resolvedLogRoot "$timestamp-$safeStage-$identifier.stdout.log"
    $stderrLog = Join-Path $resolvedLogRoot "$timestamp-$safeStage-$identifier.stderr.log"
    $renderedArguments = ConvertTo-OdsWixCommandLine -Arguments $Arguments
    $command = '"' + $resolvedExecutable + '" ' + $renderedArguments
    $encoding = [Text.UTF8Encoding]::new($false)
    $stdout = ''
    $stderr = ''
    $exitCode = $null
    $startFailure = $null

    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $resolvedExecutable
    $startInfo.Arguments = $renderedArguments
    $startInfo.WorkingDirectory = $resolvedWorkingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.StandardOutputEncoding = $encoding
    $startInfo.StandardErrorEncoding = $encoding

    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        try {
            if (-not $process.Start()) { throw 'Process.Start retornou false.' }
            $stdoutTask = $process.StandardOutput.ReadToEndAsync()
            $stderrTask = $process.StandardError.ReadToEndAsync()
            $process.WaitForExit()
            $stdout = $stdoutTask.GetAwaiter().GetResult()
            $stderr = $stderrTask.GetAwaiter().GetResult()
            $exitCode = $process.ExitCode
        } catch {
            $startFailure = $_.Exception.Message
            $stderr = $startFailure
        }
    } finally {
        $process.Dispose()
    }

    [IO.File]::WriteAllText($stdoutLog, $stdout, $encoding)
    [IO.File]::WriteAllText($stderrLog, $stderr, $encoding)

    if ($stdout) { Write-Host $stdout.TrimEnd() }
    if ($stderr -and $null -ne $exitCode) { [Console]::Error.WriteLine($stderr.TrimEnd()) }

    if ($null -eq $exitCode -or $exitCode -ne 0) {
        $reportedExitCode = if ($null -eq $exitCode) { 'processo não iniciado' } else { [string]$exitCode }
        $message = @"
WiX falhou durante a etapa '$Stage'.

Executável:
$resolvedExecutable

Versão esperada:
$ExpectedVersion

Diretório:
$resolvedWorkingDirectory

Comando:
$command

Código de saída:
$reportedExitCode

STDOUT:
$(if ($stdout) { $stdout.TrimEnd() } else { '<vazio>' })

STDERR:
$(if ($stderr) { $stderr.TrimEnd() } else { '<vazio>' })

Logs:
$stdoutLog
$stderrLog
"@
        throw $message.TrimEnd()
    }

    if (-not $PreserveSuccessLogs) {
        Remove-Item -LiteralPath $stdoutLog, $stderrLog -Force
        $stdoutLog = $null
        $stderrLog = $null
    }

    return [pscustomobject]@{
        ExitCode = $exitCode
        StdOut = $stdout
        StdErr = $stderr
        Command = $command
        StdOutLog = $stdoutLog
        StdErrLog = $stderrLog
    }
}

function Test-OdsWixExecutable {
    param(
        [Parameter(Mandatory)][string]$Executable,
        [Parameter(Mandatory)][string]$WorkingDirectory,
        [Parameter(Mandatory)][string]$LogRoot
    )

    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) { return $false }
    try {
        $result = Invoke-OdsWixProcess `
            -Stage 'version' `
            -Executable $Executable `
            -Arguments @('--version') `
            -WorkingDirectory $WorkingDirectory `
            -LogRoot $LogRoot
        return $result.StdOut.Trim() -match '^4\.0\.6(?:\+|$)'
    } catch {
        return $false
    }
}

function Resolve-OdsWixExecutable {
    [CmdletBinding()]
    param(
        [string]$WixExecutable,
        [Parameter(Mandatory)][string]$ToolsRoot,
        [Parameter(Mandatory)][string]$RepositoryRoot,
        [Parameter(Mandatory)][string]$LogRoot,
        [switch]$AllowMissing
    )

    $resolvedRepositoryRoot = [IO.Path]::GetFullPath($RepositoryRoot)
    $resolvedToolsRoot = Resolve-OdsWixAbsolutePath -Path $ToolsRoot -BasePath $resolvedRepositoryRoot

    if ($WixExecutable) {
        $explicit = Resolve-OdsWixAbsolutePath -Path $WixExecutable -BasePath $resolvedRepositoryRoot
        if (-not (Test-OdsWixExecutable -Executable $explicit -WorkingDirectory $resolvedRepositoryRoot -LogRoot $LogRoot)) {
            throw "-WixExecutable não aponta para WiX $script:OdsRequiredWixVersion válido: '$explicit'."
        }
        return $explicit
    }

    if ($env:ODS_WIX_EXE) {
        try {
            $environmentCandidate = Resolve-OdsWixAbsolutePath -Path $env:ODS_WIX_EXE -BasePath $resolvedRepositoryRoot
            if (Test-OdsWixExecutable -Executable $environmentCandidate -WorkingDirectory $resolvedRepositoryRoot -LogRoot $LogRoot) {
                return $environmentCandidate
            }
            Write-Warning "ODS_WIX_EXE não aponta para WiX $script:OdsRequiredWixVersion válido; procurando alternativa local: '$environmentCandidate'."
        } catch {
            Write-Warning "ODS_WIX_EXE é inválido; procurando alternativa local: '$env:ODS_WIX_EXE'."
        }
    }

    $localCandidate = Join-Path $resolvedToolsRoot "wix\$script:OdsRequiredWixVersion\wix.exe"
    if (Test-OdsWixExecutable -Executable $localCandidate -WorkingDirectory $resolvedRepositoryRoot -LogRoot $LogRoot) {
        return [IO.Path]::GetFullPath($localCandidate)
    }

    $pathCommand = Get-Command 'wix.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($pathCommand -and
        (Test-OdsWixExecutable -Executable $pathCommand.Source -WorkingDirectory $resolvedRepositoryRoot -LogRoot $LogRoot)) {
        return [IO.Path]::GetFullPath($pathCommand.Source)
    }

    if ($AllowMissing) { return $null }
    throw "WiX Toolset $script:OdsRequiredWixVersion não foi encontrado por parâmetro, ODS_WIX_EXE, '$localCandidate' ou PATH."
}

function Test-OdsDotNetSdk {
    param(
        [Parameter(Mandatory)][string]$Executable,
        [Parameter(Mandatory)][string]$WorkingDirectory,
        [Parameter(Mandatory)][string]$LogRoot
    )

    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) { return $false }
    try {
        $result = Invoke-OdsWixProcess `
            -Stage 'dotnet-list-sdks' `
            -Executable $Executable `
            -Arguments @('--list-sdks') `
            -WorkingDirectory $WorkingDirectory `
            -LogRoot $LogRoot `
            -ExpectedVersion 'SDK .NET disponível'
        return -not [string]::IsNullOrWhiteSpace($result.StdOut)
    } catch {
        return $false
    }
}

function Resolve-OdsDotNetExecutable {
    [CmdletBinding()]
    param(
        [string]$DotNetExecutable,
        [Parameter(Mandatory)][string]$ToolsRoot,
        [Parameter(Mandatory)][string]$RepositoryRoot,
        [Parameter(Mandatory)][string]$LogRoot
    )

    $resolvedRepositoryRoot = [IO.Path]::GetFullPath($RepositoryRoot)
    $resolvedToolsRoot = Resolve-OdsWixAbsolutePath -Path $ToolsRoot -BasePath $resolvedRepositoryRoot
    if ($DotNetExecutable) {
        $explicit = Resolve-OdsWixAbsolutePath -Path $DotNetExecutable -BasePath $resolvedRepositoryRoot
        if (-not (Test-OdsDotNetSdk -Executable $explicit -WorkingDirectory $resolvedRepositoryRoot -LogRoot $LogRoot)) {
            throw "-DotNetExecutable não aponta para um dotnet com SDK disponível: '$explicit'."
        }
        return $explicit
    }

    $localDotNetRoot = Join-Path $resolvedToolsRoot 'dotnet'
    $localCandidates = @(Get-ChildItem -LiteralPath $localDotNetRoot -Recurse -File -Filter 'dotnet.exe' -ErrorAction SilentlyContinue |
        Sort-Object FullName -Descending | Select-Object -ExpandProperty FullName)
    foreach ($candidate in $localCandidates) {
        if (Test-OdsDotNetSdk -Executable $candidate -WorkingDirectory $resolvedRepositoryRoot -LogRoot $LogRoot) {
            return [IO.Path]::GetFullPath($candidate)
        }
    }

    $pathCommand = Get-Command 'dotnet.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($pathCommand -and
        (Test-OdsDotNetSdk -Executable $pathCommand.Source -WorkingDirectory $resolvedRepositoryRoot -LogRoot $LogRoot)) {
        return [IO.Path]::GetFullPath($pathCommand.Source)
    }

    throw 'Nenhum SDK .NET foi encontrado. Forneça -DotNetExecutable ou um wix.exe 4.0.6 já provisionado; instalação global e download automático de SDK não são executados.'
}

function Move-OdsWixDirectoryToBackup {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$BackupRoot,
        [Parameter(Mandatory)][string]$Nature,
        [string]$Reason = 'incompleto'
    )

    if (-not (Test-Path -LiteralPath $Path)) { return $null }
    $resolvedBackupRoot = [IO.Path]::GetFullPath($BackupRoot)
    [void](New-Item -ItemType Directory -Path $resolvedBackupRoot -Force)
    $safeNature = [regex]::Replace($Nature, '[^A-Za-z0-9._-]', '-')
    $safeReason = [regex]::Replace($Reason, '[^A-Za-z0-9._-]', '-')
    $name = "$safeNature-$script:OdsRequiredWixVersion-$safeReason-$([DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss'))-$([guid]::NewGuid().ToString('N'))"
    $destination = Join-Path $resolvedBackupRoot $name
    Move-Item -LiteralPath $Path -Destination $destination
    Write-Warning "Diretório anterior preservado em '$destination'."
    return $destination
}

function Get-OdsWixExtensionMetadata {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$ExtensionRoot,
        [Parameter(Mandatory)][ValidateSet('WixToolset.Firewall.wixext', 'WixToolset.Util.wixext')][string]$PackageId
    )

    $resolvedRoot = [IO.Path]::GetFullPath($ExtensionRoot)
    $packageRoot = Join-Path $resolvedRoot "$PackageId\$script:OdsRequiredWixVersion"
    $markerPath = Join-Path $packageRoot '.ods-extension.json'
    if (-not (Test-Path -LiteralPath $packageRoot -PathType Container) -or
        -not (Test-Path -LiteralPath $markerPath -PathType Leaf)) {
        throw "Extensão $PackageId $script:OdsRequiredWixVersion não está provisionada completamente em '$packageRoot'."
    }

    try {
        $marker = Get-Content -LiteralPath $markerPath -Raw | ConvertFrom-Json
    } catch {
        throw "Marcador da extensão $PackageId é inválido: '$markerPath'."
    }
    if ($marker.PackageId -cne $PackageId -or $marker.Version -cne $script:OdsRequiredWixVersion) {
        throw "Marcador da extensão diverge do pacote esperado $PackageId $script:OdsRequiredWixVersion."
    }

    $expectedName = "$PackageId.dll"
    $candidates = @(Get-ChildItem -LiteralPath $packageRoot -Recurse -File -Filter $expectedName)
    if ($candidates.Count -ne 1) {
        throw "Extensão $PackageId deve conter exatamente uma DLL '$expectedName'; encontradas: $($candidates.Count)."
    }
    $dllPath = [IO.Path]::GetFullPath($candidates[0].FullName)
    $markedDll = Resolve-OdsWixAbsolutePath -Path ([string]$marker.DllRelativePath) -BasePath $packageRoot
    if (-not $dllPath.Equals($markedDll, [StringComparison]::OrdinalIgnoreCase)) {
        throw "DLL da extensão $PackageId diverge do marcador de publicação completa."
    }

    return [pscustomobject]@{
        PackageId = $PackageId
        Version = $script:OdsRequiredWixVersion
        PackageRoot = [IO.Path]::GetFullPath($packageRoot)
        DllPath = $dllPath
        MarkerPath = [IO.Path]::GetFullPath($markerPath)
    }
}

function Resolve-OdsWixExtensions {
    [CmdletBinding()]
    param(
        [string]$WixExtensionRoot,
        [Parameter(Mandatory)][string]$ToolsRoot,
        [Parameter(Mandatory)][string]$RepositoryRoot
    )

    $resolvedRepositoryRoot = [IO.Path]::GetFullPath($RepositoryRoot)
    $resolvedToolsRoot = Resolve-OdsWixAbsolutePath -Path $ToolsRoot -BasePath $resolvedRepositoryRoot
    $root = if ($WixExtensionRoot) {
        Resolve-OdsWixAbsolutePath -Path $WixExtensionRoot -BasePath $resolvedRepositoryRoot
    } else {
        Join-Path $resolvedToolsRoot 'wix-extensions'
    }
    $firewall = Get-OdsWixExtensionMetadata -ExtensionRoot $root -PackageId $script:OdsFirewallExtensionId
    $util = Get-OdsWixExtensionMetadata -ExtensionRoot $root -PackageId $script:OdsUtilExtensionId
    return [pscustomobject]@{
        ExtensionRoot = [IO.Path]::GetFullPath($root)
        FirewallExtension = $firewall.DllPath
        UtilExtension = $util.DllPath
    }
}

function Get-OdsNuGetPackageUri {
    param(
        [Parameter(Mandatory)][ValidateSet('WixToolset.Firewall.wixext', 'WixToolset.Util.wixext')][string]$PackageId
    )

    $lowerId = $PackageId.ToLowerInvariant()
    return "https://api.nuget.org/v3-flatcontainer/$lowerId/$script:OdsRequiredWixVersion/$lowerId.$script:OdsRequiredWixVersion.nupkg"
}

function Install-OdsLocalWixCli {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$DotNetExecutable,
        [Parameter(Mandatory)][string]$ToolsRoot,
        [Parameter(Mandatory)][string]$RepositoryRoot,
        [Parameter(Mandatory)][string]$StagingRoot,
        [Parameter(Mandatory)][string]$BackupRoot,
        [Parameter(Mandatory)][string]$LogRoot
    )

    $target = Join-Path $ToolsRoot "wix\$script:OdsRequiredWixVersion"
    if (Test-Path -LiteralPath $target) {
        $null = Move-OdsWixDirectoryToBackup -Path $target -BackupRoot $BackupRoot -Nature 'wix-cli'
    }
    [void](New-Item -ItemType Directory -Path $StagingRoot -Force)
    $staging = Join-Path $StagingRoot ("wix-cli-$([guid]::NewGuid().ToString('N'))")
    [void](New-Item -ItemType Directory -Path $staging)
    try {
        $null = Invoke-OdsWixProcess `
            -Stage 'dotnet-tool-install-wix' `
            -Executable $DotNetExecutable `
            -Arguments @('tool', 'install', 'wix', '--version', $script:OdsRequiredWixVersion, '--tool-path', $staging) `
            -WorkingDirectory $RepositoryRoot `
            -LogRoot $LogRoot `
            -ExpectedVersion $script:OdsRequiredWixVersion
        $stagedWix = Join-Path $staging 'wix.exe'
        if (-not (Test-OdsWixExecutable -Executable $stagedWix -WorkingDirectory $RepositoryRoot -LogRoot $LogRoot)) {
            throw "dotnet tool install não produziu WiX $script:OdsRequiredWixVersion válido."
        }
        [void](New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force)
        Move-Item -LiteralPath $staging -Destination $target
    } catch {
        if (Test-Path -LiteralPath $staging) {
            $null = Move-OdsWixDirectoryToBackup -Path $staging -BackupRoot $BackupRoot -Nature 'wix-cli-staging'
        }
        throw
    }
    return Join-Path $target 'wix.exe'
}

function Install-OdsWixExtension {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][ValidateSet('WixToolset.Firewall.wixext', 'WixToolset.Util.wixext')][string]$PackageId,
        [Parameter(Mandatory)][string]$ExtensionRoot,
        [Parameter(Mandatory)][string]$DownloadsRoot,
        [Parameter(Mandatory)][string]$StagingRoot,
        [Parameter(Mandatory)][string]$BackupRoot,
        [switch]$ForceRefresh
    )

    $packageRoot = Join-Path $ExtensionRoot "$PackageId\$script:OdsRequiredWixVersion"
    if (-not $ForceRefresh) {
        try { return Get-OdsWixExtensionMetadata -ExtensionRoot $ExtensionRoot -PackageId $PackageId } catch {}
    }
    if (Test-Path -LiteralPath $packageRoot) {
        $reason = if ($ForceRefresh) { 'refresh' } else { 'incompleto' }
        $null = Move-OdsWixDirectoryToBackup -Path $packageRoot -BackupRoot $BackupRoot -Nature $PackageId -Reason $reason
    }

    [void](New-Item -ItemType Directory -Path $DownloadsRoot, $StagingRoot -Force)
    $lowerId = $PackageId.ToLowerInvariant()
    $download = Join-Path $DownloadsRoot "$lowerId.$script:OdsRequiredWixVersion.nupkg"
    if ($ForceRefresh -and (Test-Path -LiteralPath $download -PathType Leaf)) {
        $preservedDownload = "$download.refresh-$([DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss'))-$([guid]::NewGuid().ToString('N'))"
        Move-Item -LiteralPath $download -Destination $preservedDownload
    }
    if (-not (Test-Path -LiteralPath $download -PathType Leaf) -or (Get-Item -LiteralPath $download).Length -eq 0) {
        if (Test-Path -LiteralPath $download) {
            $invalidDownload = "$download.invalid-$([guid]::NewGuid().ToString('N'))"
            Move-Item -LiteralPath $download -Destination $invalidDownload
        }
        $partial = "$download.$([guid]::NewGuid().ToString('N')).partial"
        try {
            Invoke-WebRequest -Uri (Get-OdsNuGetPackageUri -PackageId $PackageId) -OutFile $partial -UseBasicParsing
            if (-not (Test-Path -LiteralPath $partial -PathType Leaf) -or (Get-Item -LiteralPath $partial).Length -eq 0) {
                throw "Download NuGet de $PackageId retornou arquivo vazio."
            }
            Move-Item -LiteralPath $partial -Destination $download
        } catch {
            if (Test-Path -LiteralPath $partial) {
                $failedDownload = "$partial.failed"
                Move-Item -LiteralPath $partial -Destination $failedDownload
            }
            throw "Falha ao baixar $PackageId $script:OdsRequiredWixVersion do NuGet oficial: $($_.Exception.Message)"
        }
    }

    $staging = Join-Path $StagingRoot ("$lowerId-$([guid]::NewGuid().ToString('N'))")
    $extract = Join-Path $staging 'package'
    [void](New-Item -ItemType Directory -Path $extract -Force)
    try {
        $zipCopy = Join-Path $staging "$lowerId.zip"
        Copy-Item -LiteralPath $download -Destination $zipCopy
        Expand-Archive -LiteralPath $zipCopy -DestinationPath $extract

        $nuspecs = @(Get-ChildItem -LiteralPath $extract -Recurse -File -Filter '*.nuspec')
        if ($nuspecs.Count -ne 1) { throw "Pacote deve conter exatamente um .nuspec; encontrados: $($nuspecs.Count)." }
        [xml]$nuspec = Get-Content -LiteralPath $nuspecs[0].FullName -Raw
        $idNode = $nuspec.SelectSingleNode("/*[local-name()='package']/*[local-name()='metadata']/*[local-name()='id']")
        $versionNode = $nuspec.SelectSingleNode("/*[local-name()='package']/*[local-name()='metadata']/*[local-name()='version']")
        if ($null -eq $idNode -or $idNode.InnerText -cne $PackageId) { throw 'ID do pacote NuGet é divergente.' }
        if ($null -eq $versionNode -or $versionNode.InnerText -cne $script:OdsRequiredWixVersion) { throw 'Versão do pacote NuGet é divergente.' }

        $expectedDllName = "$PackageId.dll"
        $dlls = @(Get-ChildItem -LiteralPath $extract -Recurse -File -Filter $expectedDllName)
        if ($dlls.Count -ne 1) { throw "Pacote deve conter exatamente uma DLL '$expectedDllName'; encontradas: $($dlls.Count)." }
        $relativeDll = $dlls[0].FullName.Substring($extract.Length).TrimStart('\', '/')
        $marker = [ordered]@{
            PackageId = $PackageId
            Version = $script:OdsRequiredWixVersion
            DllRelativePath = $relativeDll
            PackageSha256 = (Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash
        } | ConvertTo-Json
        [IO.File]::WriteAllText((Join-Path $extract '.ods-extension.json'), $marker, [Text.UTF8Encoding]::new($false))

        [void](New-Item -ItemType Directory -Path (Split-Path -Parent $packageRoot) -Force)
        Move-Item -LiteralPath $extract -Destination $packageRoot
        return Get-OdsWixExtensionMetadata -ExtensionRoot $ExtensionRoot -PackageId $PackageId
    } catch {
        throw "Pacote $PackageId $script:OdsRequiredWixVersion é inválido: $($_.Exception.Message)"
    } finally {
        if (Test-Path -LiteralPath $staging) { Remove-Item -LiteralPath $staging -Recurse -Force }
    }
}

function Initialize-OdsWixTooling {
    [CmdletBinding()]
    param(
        [string]$WixExecutable,
        [string]$DotNetExecutable,
        [string]$ToolsRoot,
        [switch]$ForceRefresh,
        [Parameter(Mandatory)][string]$RepositoryRoot
    )

    $resolvedRepositoryRoot = [IO.Path]::GetFullPath($RepositoryRoot)
    if (-not $ToolsRoot) { $ToolsRoot = Join-Path $resolvedRepositoryRoot '.local-data\tools' }
    $resolvedToolsRoot = Resolve-OdsWixAbsolutePath -Path $ToolsRoot -BasePath $resolvedRepositoryRoot
    $localDataRoot = [IO.Path]::GetFullPath((Join-Path $resolvedRepositoryRoot '.local-data'))
    $logRoot = Join-Path $localDataRoot 'logs\wix'
    $stagingRoot = Join-Path $localDataRoot 'staging'
    $downloadsRoot = Join-Path $localDataRoot 'downloads'
    $backupRoot = Join-Path $localDataRoot 'backups\tools'
    $extensionRoot = Join-Path $resolvedToolsRoot 'wix-extensions'

    $resolvedWix = Resolve-OdsWixExecutable `
        -WixExecutable $WixExecutable `
        -ToolsRoot $resolvedToolsRoot `
        -RepositoryRoot $resolvedRepositoryRoot `
        -LogRoot $logRoot `
        -AllowMissing
    if (-not $resolvedWix) {
        $resolvedDotNet = Resolve-OdsDotNetExecutable `
            -DotNetExecutable $DotNetExecutable `
            -ToolsRoot $resolvedToolsRoot `
            -RepositoryRoot $resolvedRepositoryRoot `
            -LogRoot $logRoot
        $resolvedWix = Install-OdsLocalWixCli `
            -DotNetExecutable $resolvedDotNet `
            -ToolsRoot $resolvedToolsRoot `
            -RepositoryRoot $resolvedRepositoryRoot `
            -StagingRoot $stagingRoot `
            -BackupRoot $backupRoot `
            -LogRoot $logRoot
    }

    $firewall = Install-OdsWixExtension `
        -PackageId $script:OdsFirewallExtensionId `
        -ExtensionRoot $extensionRoot `
        -DownloadsRoot $downloadsRoot `
        -StagingRoot $stagingRoot `
        -BackupRoot $backupRoot `
        -ForceRefresh:$ForceRefresh
    $util = Install-OdsWixExtension `
        -PackageId $script:OdsUtilExtensionId `
        -ExtensionRoot $extensionRoot `
        -DownloadsRoot $downloadsRoot `
        -StagingRoot $stagingRoot `
        -BackupRoot $backupRoot `
        -ForceRefresh:$ForceRefresh

    $versionResult = Invoke-OdsWixProcess `
        -Stage 'version' `
        -Executable $resolvedWix `
        -Arguments @('--version') `
        -WorkingDirectory $resolvedRepositoryRoot `
        -LogRoot $logRoot
    $detectedVersion = $versionResult.StdOut.Trim()
    if ($detectedVersion -notmatch '^4\.0\.6(?:\+|$)') {
        throw "WiX $script:OdsRequiredWixVersion é obrigatório; detectado: '$detectedVersion'."
    }

    return [pscustomobject]@{
        WixExecutable = [IO.Path]::GetFullPath($resolvedWix)
        WixVersion = $detectedVersion
        ExtensionRoot = [IO.Path]::GetFullPath($extensionRoot)
        FirewallExtension = $firewall.DllPath
        UtilExtension = $util.DllPath
        ToolsRoot = $resolvedToolsRoot
    }
}
