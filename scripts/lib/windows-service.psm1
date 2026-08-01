Set-StrictMode -Version Latest

function ConvertTo-OdsWindowsNativeArgument {
    [CmdletBinding()]
    param([AllowEmptyString()][AllowNull()][string]$Argument)

    if ($null -eq $Argument -or $Argument.Length -eq 0) { return '""' }
    if ($Argument -notmatch '[\s"]') { return $Argument }

    $builder = [Text.StringBuilder]::new()
    [void]$builder.Append('"')
    $backslashes = 0
    foreach ($character in $Argument.ToCharArray()) {
        if ($character -eq '\') {
            $backslashes++
            continue
        }
        if ($character -eq '"') {
            [void]$builder.Append(('\' * (($backslashes * 2) + 1)))
            [void]$builder.Append('"')
            $backslashes = 0
            continue
        }
        if ($backslashes -gt 0) {
            [void]$builder.Append(('\' * $backslashes))
            $backslashes = 0
        }
        [void]$builder.Append($character)
    }
    if ($backslashes -gt 0) { [void]$builder.Append(('\' * ($backslashes * 2))) }
    [void]$builder.Append('"')
    return $builder.ToString()
}

function Join-OdsWindowsNativeArguments {
    [CmdletBinding()]
    param([AllowEmptyCollection()][string[]]$Arguments = @())

    return (($Arguments | ForEach-Object { ConvertTo-OdsWindowsNativeArgument $_ }) -join ' ')
}

function Invoke-OdsNativeProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [AllowEmptyCollection()][string[]]$Arguments = @(),
        [int[]]$AcceptedExitCodes = @(0),
        [scriptblock]$EventWriter,
        [hashtable]$EventData = @{}
    )

    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $FilePath
    $startInfo.Arguments = Join-OdsWindowsNativeArguments $Arguments
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true

    try {
        $encoding = [Text.Encoding]::GetEncoding([Globalization.CultureInfo]::CurrentCulture.TextInfo.OEMCodePage)
        if ($startInfo.PSObject.Properties.Name -contains 'StandardOutputEncoding') {
            $startInfo.StandardOutputEncoding = $encoding
            $startInfo.StandardErrorEncoding = $encoding
        }
    } catch {
        # A codificação padrão do processo é mantida quando a página OEM não está disponível.
    }

    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        if (-not $process.Start()) { throw "Não foi possível iniciar '$FilePath'." }
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $stderr = $stderrTask.GetAwaiter().GetResult()
        $exitCode = $process.ExitCode
    } finally {
        $process.Dispose()
    }

    $combinedOutput = (@($stdout, $stderr) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) -join "`n"
    if ($combinedOutput.Length -gt 2048) { $combinedOutput = $combinedOutput.Substring(0, 2048) }
    if ($EventWriter) {
        $data = @{}
        foreach ($key in $EventData.Keys) { $data[$key] = $EventData[$key] }
        $data.nativeExecutablePath = $FilePath
        if (-not $data.ContainsKey('executablePath')) { $data.executablePath = $FilePath }
        $data.argumentCount = $Arguments.Count
        $data.exitCode = $exitCode
        if ($exitCode -notin $AcceptedExitCodes -and $combinedOutput) { $data.output = $combinedOutput.Trim() }
        & $EventWriter $(if ($exitCode -in $AcceptedExitCodes) { 'INFO' } else { 'ERROR' }) `
            'SERVICE_CONTROL_COMMAND' `
            "Processo nativo finalizado com código $exitCode." `
            $data
    }
    if ($exitCode -notin $AcceptedExitCodes) {
        $operation = if ($Arguments.Count -gt 0) { $Arguments[0] } else { 'execução' }
        throw "Falha na operação nativa '$operation' (código $exitCode)."
    }
    return [pscustomobject]@{ ExitCode = $exitCode; StandardOutput = $stdout; StandardError = $stderr }
}

function Start-OdsProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [AllowEmptyCollection()][string[]]$Arguments = @(),
        [string]$Verb,
        [switch]$Wait,
        [switch]$PassThru
    )

    $parameters = @{
        FilePath = $FilePath
        ArgumentList = (Join-OdsWindowsNativeArguments $Arguments)
    }
    if ($Verb) { $parameters.Verb = $Verb }
    if ($Wait) { $parameters.Wait = $true }
    if ($PassThru) { $parameters.PassThru = $true }
    return Start-Process @parameters
}

function Get-OdsWindowsServiceSnapshot {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$ServiceName)

    $registryPath = "HKLM:\SYSTEM\CurrentControlSet\Services\$ServiceName"
    if (-not (Test-Path -LiteralPath $registryPath)) {
        return [pscustomobject]@{ Exists = $false; Name = $ServiceName; Status = 'Absent' }
    }
    $properties = Get-ItemProperty -LiteralPath $registryPath
    $service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
    $delayedAutoStart = $properties.PSObject.Properties['DelayedAutoStart']
    $serviceSidType = $properties.PSObject.Properties['ServiceSidType']
    return [pscustomobject]@{
        Exists = $true
        Name = $ServiceName
        Status = if ($service) { [string]$service.Status } else { 'Unknown' }
        ImagePath = [string]$properties.ImagePath
        ObjectName = [string]$properties.ObjectName
        DisplayName = [string]$properties.DisplayName
        Start = [int]$properties.Start
        DelayedAutoStart = if ($null -ne $delayedAutoStart) { [int]$delayedAutoStart.Value } else { 0 }
        ServiceSidType = if ($null -ne $serviceSidType) { [int]$serviceSidType.Value } else { 0 }
    }
}

function Assert-OdsWindowsServiceCompatible {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Snapshot,
        [Parameter(Mandatory)][string]$ServiceName,
        [Parameter(Mandatory)][string]$InstallRoot
    )

    if (-not $Snapshot.Exists) { return }
    if (-not ([string]$Snapshot.ObjectName).Equals('NT AUTHORITY\LocalService', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Já existe um serviço '$ServiceName' incompatível, configurado para outra conta."
    }
    $match = [regex]::Match([string]$Snapshot.ImagePath, '^"([^"]+)" --service$')
    if (-not $match.Success) {
        throw "Já existe um serviço '$ServiceName' com comando incompatível."
    }
    $controlledRoot = [IO.Path]::GetFullPath((Join-Path $InstallRoot 'versions')).TrimEnd('\') + '\'
    $existingExecutable = [IO.Path]::GetFullPath($match.Groups[1].Value)
    if (-not $existingExecutable.StartsWith($controlledRoot, [StringComparison]::OrdinalIgnoreCase) -or
        [IO.Path]::GetFileName($existingExecutable) -ne 'offline-dental-system.exe' -or
        -not (Test-Path -LiteralPath $existingExecutable -PathType Leaf)) {
        throw "Já existe um serviço '$ServiceName' fora do diretório controlado pelo produto."
    }
}

function Invoke-OdsServiceControl {
    param(
        [Parameter(Mandatory)][string[]]$Arguments,
        [int[]]$AcceptedCodes = @(0),
        [Parameter(Mandatory)][scriptblock]$NativeInvoker,
        [hashtable]$Metadata = @{}
    )
    $operationMetadata = @{}
    foreach ($key in $Metadata.Keys) { $operationMetadata[$key] = $Metadata[$key] }
    $operationMetadata.operation = if ($Arguments.Count -gt 0) { $Arguments[0] } else { 'unknown' }
    & $NativeInvoker -Arguments $Arguments -AcceptedCodes $AcceptedCodes -Metadata $operationMetadata
}

function Wait-OdsServiceStatus {
    param(
        [Parameter(Mandatory)][string]$ExpectedStatus,
        [Parameter(Mandatory)][scriptblock]$SnapshotProvider,
        [int]$TimeoutSeconds = 30
    )
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $snapshot = & $SnapshotProvider
        if ($snapshot.Exists -and ([string]$snapshot.Status).Equals($ExpectedStatus, [StringComparison]::OrdinalIgnoreCase)) { return }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "O serviço não atingiu o estado '$ExpectedStatus' no tempo esperado."
}

function Set-OdsWindowsService {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$ServiceName,
        [Parameter(Mandatory)][string]$ProductName,
        [Parameter(Mandatory)][string]$Executable,
        [Parameter(Mandatory)][string]$InstallRoot,
        [Parameter(Mandatory)][ref]$CreatedThisRun,
        [Parameter(Mandatory)][ref]$ChangedThisRun,
        [scriptblock]$NativeInvoker,
        [scriptblock]$SnapshotProvider
    )

    if (-not $NativeInvoker) {
        $sc = Join-Path $env:SystemRoot 'System32\sc.exe'
        $NativeInvoker = { param($Arguments, $AcceptedCodes, $Metadata) Invoke-OdsNativeProcess -FilePath $sc -Arguments $Arguments -AcceptedExitCodes $AcceptedCodes -EventData $Metadata }.GetNewClosure()
    }
    if (-not $SnapshotProvider) { $SnapshotProvider = { Get-OdsWindowsServiceSnapshot -ServiceName $ServiceName }.GetNewClosure() }

    $initial = & $SnapshotProvider
    Assert-OdsWindowsServiceCompatible -Snapshot $initial -ServiceName $ServiceName -InstallRoot $InstallRoot
    $imagePath = '"' + [IO.Path]::GetFullPath($Executable) + '" --service'
    $metadata = @{
        operation = 'configure-service'
        serviceName = $ServiceName
        serviceArguments = '--service'
        account = 'NT AUTHORITY\LocalService'
        displayName = $ProductName
        startMode = 'delayed-auto'
        executablePath = [IO.Path]::GetFullPath($Executable)
    }

    if (-not $initial.Exists) {
        Invoke-OdsServiceControl -NativeInvoker $NativeInvoker -Metadata $metadata -Arguments @(
            'create', $ServiceName,
            'binPath=', $imagePath,
            'start=', 'auto',
            'error=', 'normal',
            'obj=', 'NT AUTHORITY\LocalService',
            'DisplayName=', $ProductName
        )
        $CreatedThisRun.Value = $true
        $created = & $SnapshotProvider
        if (-not $created.Exists) { throw 'O Service Control Manager não confirmou a criação do serviço.' }
    } else {
        $ChangedThisRun.Value = $true
        if (-not ([string]$initial.Status).Equals('Stopped', [StringComparison]::OrdinalIgnoreCase)) {
            Invoke-OdsServiceControl -NativeInvoker $NativeInvoker -Metadata $metadata -Arguments @('stop', $ServiceName) -AcceptedCodes @(0, 1062)
            Wait-OdsServiceStatus -ExpectedStatus 'Stopped' -SnapshotProvider $SnapshotProvider
        }
        Invoke-OdsServiceControl -NativeInvoker $NativeInvoker -Metadata $metadata -Arguments @(
            'config', $ServiceName,
            'binPath=', $imagePath,
            'start=', 'auto',
            'error=', 'normal',
            'obj=', 'NT AUTHORITY\LocalService',
            'DisplayName=', $ProductName
        )
    }

    Invoke-OdsServiceControl -NativeInvoker $NativeInvoker -Metadata $metadata -Arguments @('description', $ServiceName, 'Servidor web local e cifrado para a clínica.')
    Invoke-OdsServiceControl -NativeInvoker $NativeInvoker -Metadata $metadata -Arguments @('sidtype', $ServiceName, 'restricted')
    Invoke-OdsServiceControl -NativeInvoker $NativeInvoker -Metadata $metadata -Arguments @('config', $ServiceName, 'start=', 'delayed-auto', 'error=', 'normal')
    Invoke-OdsServiceControl -NativeInvoker $NativeInvoker -Metadata $metadata -Arguments @('failure', $ServiceName, 'reset=', '86400', 'actions=', 'restart/15000/restart/15000/restart/15000')

    $verified = & $SnapshotProvider
    if (-not $verified.Exists -or
        [string]$verified.ImagePath -cne $imagePath -or
        -not ([string]$verified.ObjectName).Equals('NT AUTHORITY\LocalService', [StringComparison]::OrdinalIgnoreCase) -or
        [int]$verified.Start -ne 2 -or [int]$verified.DelayedAutoStart -ne 1 -or [int]$verified.ServiceSidType -ne 3) {
        throw 'A configuração persistida do serviço diverge da configuração de segurança esperada.'
    }
    return $verified
}

function Undo-OdsWindowsServiceChange {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$ServiceName,
        [Parameter(Mandatory)][bool]$CreatedThisRun,
        [Parameter(Mandatory)][bool]$ChangedThisRun,
        $PreviousSnapshot,
        [scriptblock]$NativeInvoker,
        [scriptblock]$SnapshotProvider
    )

    if (-not $CreatedThisRun -and -not $ChangedThisRun) { return }
    if (-not $NativeInvoker) {
        $sc = Join-Path $env:SystemRoot 'System32\sc.exe'
        $NativeInvoker = { param($Arguments, $AcceptedCodes, $Metadata) Invoke-OdsNativeProcess -FilePath $sc -Arguments $Arguments -AcceptedExitCodes $AcceptedCodes -EventData $Metadata }.GetNewClosure()
    }
    if (-not $SnapshotProvider) { $SnapshotProvider = { Get-OdsWindowsServiceSnapshot -ServiceName $ServiceName }.GetNewClosure() }
    $metadata = @{ operation = 'rollback-service'; serviceName = $ServiceName; serviceArguments = '--service' }
    $current = & $SnapshotProvider

    if ($CreatedThisRun) {
        if ($current.Exists -and -not ([string]$current.Status).Equals('Stopped', [StringComparison]::OrdinalIgnoreCase)) {
            Invoke-OdsServiceControl -NativeInvoker $NativeInvoker -Metadata $metadata -Arguments @('stop', $ServiceName) -AcceptedCodes @(0, 1062, 1060)
        }
        if ($current.Exists) {
            Invoke-OdsServiceControl -NativeInvoker $NativeInvoker -Metadata $metadata -Arguments @('delete', $ServiceName) -AcceptedCodes @(0, 1060, 1072)
        }
        return
    }

    if ($ChangedThisRun -and $PreviousSnapshot -and $PreviousSnapshot.Exists) {
        Invoke-OdsServiceControl -NativeInvoker $NativeInvoker -Metadata $metadata -Arguments @(
            'config', $ServiceName,
            'binPath=', [string]$PreviousSnapshot.ImagePath,
            'start=', $(if ([int]$PreviousSnapshot.Start -eq 2 -and [int]$PreviousSnapshot.DelayedAutoStart -eq 1) { 'delayed-auto' } elseif ([int]$PreviousSnapshot.Start -eq 2) { 'auto' } else { 'demand' }),
            'obj=', [string]$PreviousSnapshot.ObjectName,
            'DisplayName=', [string]$PreviousSnapshot.DisplayName
        )
        if (-not ([string]$PreviousSnapshot.Status).Equals('Stopped', [StringComparison]::OrdinalIgnoreCase)) {
            Invoke-OdsServiceControl -NativeInvoker $NativeInvoker -Metadata $metadata -Arguments @('start', $ServiceName) -AcceptedCodes @(0, 1056)
        }
    }
}

Export-ModuleMember -Function @(
    'ConvertTo-OdsWindowsNativeArgument',
    'Join-OdsWindowsNativeArguments',
    'Invoke-OdsNativeProcess',
    'Start-OdsProcess',
    'Get-OdsWindowsServiceSnapshot',
    'Assert-OdsWindowsServiceCompatible',
    'Set-OdsWindowsService',
    'Undo-OdsWindowsServiceChange'
)
