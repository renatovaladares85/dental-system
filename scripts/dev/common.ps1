Set-StrictMode -Version Latest

$script:RepositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$script:CargoManifest = Join-Path $script:RepositoryRoot 'src-tauri\Cargo.toml'
$script:DevelopmentRoot = Join-Path $script:RepositoryRoot '.local-data\dev-host'
$script:DevelopmentData = Join-Path $script:DevelopmentRoot 'Data'
$script:DevelopmentPidFile = Join-Path $script:DevelopmentRoot 'server.pid'
$script:LogDirectory = Join-Path $script:RepositoryRoot '.local-data\logs'
$script:HealthUrl = 'http://127.0.0.1:8742/api/v1/health'
$script:ApplicationUrl = 'http://127.0.0.1:8742'

function Assert-OdsPowerShell7 {
    if ($PSVersionTable.PSVersion.Major -lt 7) {
        throw 'PowerShell 7 ou superior é obrigatório para os scripts de desenvolvimento.'
    }
}

function Test-OdsCommand([string]$Name) {
    return $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}

function Update-OdsProcessPath {
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $pathSeparator = [IO.Path]::PathSeparator
    $pathEntries = [Collections.Generic.List[string]]::new()
    $seenPathEntries = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($pathValue in @($machinePath, $userPath, $env:Path)) {
        if ([string]::IsNullOrWhiteSpace($pathValue)) { continue }
        foreach ($entry in $pathValue.Split($pathSeparator, [StringSplitOptions]::RemoveEmptyEntries)) {
            $trimmedEntry = $entry.Trim()
            if ($trimmedEntry -and $seenPathEntries.Add($trimmedEntry)) {
                $pathEntries.Add($trimmedEntry)
            }
        }
    }
    $env:Path = $pathEntries -join [string]$pathSeparator

    $machinePathExt = [Environment]::GetEnvironmentVariable('PATHEXT', 'Machine')
    $userPathExt = [Environment]::GetEnvironmentVariable('PATHEXT', 'User')
    $pathExtEntries = [Collections.Generic.List[string]]::new()
    $seenPathExtEntries = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($pathExtValue in @($machinePathExt, $userPathExt, $env:PATHEXT, '.COM;.EXE;.BAT;.CMD')) {
        if ([string]::IsNullOrWhiteSpace($pathExtValue)) { continue }
        foreach ($entry in $pathExtValue.Split($pathSeparator, [StringSplitOptions]::RemoveEmptyEntries)) {
            $trimmedEntry = $entry.Trim()
            if ($trimmedEntry -and $seenPathExtEntries.Add($trimmedEntry)) {
                $pathExtEntries.Add($trimmedEntry)
            }
        }
    }
    $env:PATHEXT = $pathExtEntries -join [string]$pathSeparator
}

function Test-OdsVisualStudioEnvironment {
    foreach ($command in 'cl.exe', 'link.exe', 'lib.exe', 'rc.exe') {
        if (-not (Test-OdsCommand $command)) { return $false }
    }
    if ($env:VSCMD_ARG_TGT_ARCH -cne 'x64') { return $false }
    if ($env:VSCMD_ARG_HOST_ARCH -cne 'x64') { return $false }
    return $true
}

function Get-OdsVsDeveloperCommand {
    $vswhereCandidates = @(
        (Get-Command 'vswhere.exe' -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -First 1),
        (Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe')
    ) | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) } | Select-Object -Unique

    foreach ($vswhere in $vswhereCandidates) {
        $global:LASTEXITCODE = 0
        $installation = & $vswhere -latest -products '*' `
            -requires 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64' `
            -property installationPath
        if ($LASTEXITCODE -eq 0 -and $installation) {
            $candidate = Join-Path $installation.Trim() 'Common7\Tools\VsDevCmd.bat'
            if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
        }
    }
    return $null
}

function Import-OdsVisualStudioEnvironment {
    if (Test-OdsVisualStudioEnvironment) { return }
    $developerCommand = Get-OdsVsDeveloperCommand
    if (-not $developerCommand) {
        throw 'Visual Studio C++ Build Tools com o componente x64 não foi encontrado via vswhere.exe.'
    }

    $global:LASTEXITCODE = 0
    $capture = cmd.exe /d /s /c "call `"$developerCommand`" -no_logo -arch=x64 -host_arch=x64 && set"
    if ($LASTEXITCODE -ne 0) { throw 'Falha ao carregar o ambiente MSVC x64.' }
    foreach ($line in $capture) {
        $separator = $line.IndexOf('=')
        if ($separator -le 0) { continue }
        [Environment]::SetEnvironmentVariable(
            $line.Substring(0, $separator),
            $line.Substring($separator + 1),
            'Process'
        )
    }

    if (-not (Test-OdsVisualStudioEnvironment)) {
        throw "O ambiente MSVC x64 foi carregado, mas uma ferramenta obrigatória ou arquitetura x64 não está disponível."
    }
}

function Invoke-OdsNative {
    param(
        [Parameter(Mandatory, Position = 0)][string]$FilePath,
        [int[]]$AcceptedExitCodes = @(0),
        [Parameter(ValueFromRemainingArguments, Position = 1)][string[]]$Arguments
    )

    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $FilePath
    $startInfo.WorkingDirectory = $script:RepositoryRoot
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $utf8 = [Text.UTF8Encoding]::new($false)
    $startInfo.StandardOutputEncoding = $utf8
    $startInfo.StandardErrorEncoding = $utf8
    foreach ($argument in $Arguments) {
        [void]$startInfo.ArgumentList.Add($argument)
    }

    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    $stdout = ''
    $stderr = ''
    $exitCode = $null
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

    if ($stdout) { Write-Host $stdout.TrimEnd() }
    if ($stderr) { [Console]::Error.WriteLine($stderr.TrimEnd()) }
    if ($exitCode -notin $AcceptedExitCodes) {
        throw "O comando '$FilePath' falhou com código $exitCode."
    }
    return [pscustomobject]@{ ExitCode = $exitCode; StdOut = $stdout; StdErr = $stderr }
}

function Invoke-OdsNpm {
    param(
        [Parameter(ValueFromRemainingArguments, Position = 0)][string[]]$Arguments
    )

    $npmCommand = Get-Command 'npm.cmd' -ErrorAction Stop
    $npmDirectory = Split-Path -Parent $npmCommand.Source
    $npmCli = Join-Path $npmDirectory 'node_modules\npm\bin\npm-cli.js'
    if (-not (Test-Path -LiteralPath $npmCli -PathType Leaf)) {
        throw "Não foi possível localizar npm-cli.js a partir de '$($npmCommand.Source)'."
    }
    Invoke-OdsNative 'node.exe' $npmCli @Arguments
}

function ConvertTo-OdsWindowsCommandLine([string[]]$Arguments) {
    return ($Arguments | ForEach-Object {
        $value = [string]$_
        if ($value.Length -eq 0) { return '""' }
        if ($value -notmatch '[\s"]') { return $value }
        $escaped = [regex]::Replace($value, '(\\*)"', '$1$1\\"')
        $escaped = [regex]::Replace($escaped, '(\\+)$', '$1$1')
        return '"' + $escaped + '"'
    }) -join ' '
}

function Start-OdsDevelopmentHost {
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [Parameter(Mandatory)][string[]]$Arguments,
        [Parameter(Mandatory)][string]$StandardOutputPath,
        [Parameter(Mandatory)][string]$StandardErrorPath
    )

    $process = Start-Process -FilePath $FilePath `
        -ArgumentList (ConvertTo-OdsWindowsCommandLine $Arguments) `
        -WorkingDirectory $script:RepositoryRoot `
        -RedirectStandardOutput $StandardOutputPath `
        -RedirectStandardError $StandardErrorPath `
        -NoNewWindow `
        -PassThru
    if ($null -eq $process) { throw "Não foi possível iniciar '$FilePath'." }
    return $process
}

function Write-OdsDevelopmentPid([Diagnostics.Process]$Process, [string]$Executable, [string[]]$Arguments) {
    [pscustomobject]@{
        pid = $Process.Id
        executable = [IO.Path]::GetFullPath($Executable)
        arguments = $Arguments
        dataDirectory = [IO.Path]::GetFullPath($script:DevelopmentData)
        startedAtUtc = [DateTime]::UtcNow.ToString('O')
    } | ConvertTo-Json -Compress | Set-Content -LiteralPath $script:DevelopmentPidFile -Encoding utf8 -NoNewline
}

function Remove-OdsDevelopmentPid {
    if (Test-Path -LiteralPath $script:DevelopmentPidFile -PathType Leaf) {
        Remove-Item -LiteralPath $script:DevelopmentPidFile -Force
    }
}

function ConvertFrom-OdsWindowsCommandLine([string]$CommandLine) {
    if (-not ('OdsCommandLineParser' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class OdsCommandLineParser {
    [DllImport("shell32.dll", SetLastError = true)]
    private static extern IntPtr CommandLineToArgvW(string commandLine, out int argumentCount);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr LocalFree(IntPtr memory);

    public static string[] Parse(string commandLine) {
        int argumentCount;
        IntPtr arguments = CommandLineToArgvW(commandLine, out argumentCount);
        if (arguments == IntPtr.Zero) { throw new System.ComponentModel.Win32Exception(); }
        try {
            var result = new string[argumentCount];
            for (int index = 0; index < argumentCount; index++) {
                IntPtr value = Marshal.ReadIntPtr(arguments, index * IntPtr.Size);
                result[index] = Marshal.PtrToStringUni(value);
            }
            return result;
        } finally {
            LocalFree(arguments);
        }
    }
}
'@
    }
    return [OdsCommandLineParser]::Parse($CommandLine)
}

function Test-OdsDevelopmentHostArguments {
    param(
        [Parameter(Mandatory)][string[]]$ActualArguments,
        [Parameter(Mandatory)][string]$ExpectedExecutable,
        [Parameter(Mandatory)][string]$ExpectedDataDirectory
    )

    if ($ActualArguments.Count -ne 4) { return $false }
    $expected = @(
        [IO.Path]::GetFullPath($ExpectedExecutable),
        '--console',
        '--data-directory',
        [IO.Path]::GetFullPath($ExpectedDataDirectory)
    )
    for ($index = 0; $index -lt $expected.Count; $index++) {
        if ($index -in 0, 3) {
            if (-not $ActualArguments[$index].Equals($expected[$index], [StringComparison]::OrdinalIgnoreCase)) { return $false }
        } elseif ($ActualArguments[$index] -cne $expected[$index]) {
            return $false
        }
    }
    return $true
}

function Get-OdsDevelopmentHost {
    if (-not (Test-Path -LiteralPath $script:DevelopmentPidFile -PathType Leaf)) { return $null }
    try {
        $metadata = Get-Content -LiteralPath $script:DevelopmentPidFile -Raw | ConvertFrom-Json
        $process = Get-CimInstance Win32_Process -Filter "ProcessId=$([int]$metadata.pid)" -ErrorAction SilentlyContinue
    } catch {
        return [pscustomobject]@{ State = 'invalid-pid-file'; Metadata = $null; Process = $null }
    }
    if ($null -eq $process) {
        return [pscustomobject]@{ State = 'not-running'; Metadata = $metadata; Process = $null }
    }

    try {
        $expectedExecutable = [IO.Path]::GetFullPath([string]$metadata.executable)
        $expectedData = [IO.Path]::GetFullPath([string]$metadata.dataDirectory)
        $startedAtUtc = [DateTime]::Parse([string]$metadata.startedAtUtc).ToUniversalTime()
        $commandArguments = ConvertFrom-OdsWindowsCommandLine ([string]$process.CommandLine)
        $matchesExpectedHost = $process.ExecutablePath -and $startedAtUtc -le [DateTime]::UtcNow -and
            [IO.Path]::GetFullPath([string]$process.ExecutablePath).Equals($expectedExecutable, [StringComparison]::OrdinalIgnoreCase) -and
            (Test-OdsDevelopmentHostArguments -ActualArguments $commandArguments -ExpectedExecutable $expectedExecutable -ExpectedDataDirectory $expectedData)
    } catch {
        $matchesExpectedHost = $false
    }
    if (-not $matchesExpectedHost) {
        return [pscustomobject]@{ State = 'divergent'; Metadata = $metadata; Process = $process }
    }
    return [pscustomobject]@{ State = 'running'; Metadata = $metadata; Process = $process }
}

function Test-OdsHealth {
    try {
        $response = Invoke-WebRequest -Uri $script:HealthUrl -TimeoutSec 2
        if ($response.StatusCode -ne 200 -or
            -not ([string]$response.Headers['Content-Type']).StartsWith('application/json', [StringComparison]::OrdinalIgnoreCase) -or
            -not ([string]$response.Headers['Cache-Control']).Contains('no-store')) {
            return $false
        }
        return ($response.Content | ConvertFrom-Json).status -eq 'ok'
    } catch {
        return $false
    }
}

function Assert-OdsPortsAvailable {
    foreach ($port in 8742, 8743) {
        $listeners = @(Get-NetTCPConnection -State Listen -LocalPort $port -ErrorAction SilentlyContinue)
        if ($listeners.Count -gt 0) {
            $owners = $listeners | Select-Object -ExpandProperty OwningProcess -Unique
            throw "A porta $port já pertence ao(s) PID(s) $($owners -join ', '). A instância existente deve ser investigada e encerrada explicitamente."
        }
    }
}

function Assert-OdsLocalPathWithoutReparsePoint([string]$Path) {
    $fullPath = [IO.Path]::GetFullPath($Path)
    if ($fullPath.StartsWith('\\', [StringComparison]::Ordinal)) {
        throw 'O ambiente de desenvolvimento exige um caminho local, não UNC.'
    }
    $root = [IO.Path]::GetPathRoot($fullPath)
    $drive = [IO.DriveInfo]::new($root)
    if (-not $drive.IsReady -or $drive.DriveType -ne [IO.DriveType]::Fixed) {
        throw 'O ambiente de desenvolvimento exige uma unidade local fixa.'
    }
    $current = $fullPath
    while (-not (Test-Path -LiteralPath $current)) { $current = Split-Path -Parent $current }
    while ($current) {
        if (((Get-Item -LiteralPath $current -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "O caminho de desenvolvimento atravessa um reparse point: '$current'."
        }
        $current = Split-Path -Parent $current
    }
}
