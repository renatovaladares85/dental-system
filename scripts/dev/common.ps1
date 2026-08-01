Set-StrictMode -Version Latest

$script:RepositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$script:CargoManifest = Join-Path $script:RepositoryRoot 'src-tauri\Cargo.toml'
$script:DevelopmentRoot = Join-Path $script:RepositoryRoot '.local-data\dev-host'
$script:DevelopmentData = Join-Path $script:DevelopmentRoot 'Data'
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

function Invoke-OdsNative {
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [Parameter(ValueFromRemainingArguments)][string[]]$Arguments,
        [int[]]$AcceptedExitCodes = @(0)
    )

    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $FilePath
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in $Arguments) {
        [void]$startInfo.ArgumentList.Add($argument)
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

    if ($stdout) { Write-Host $stdout.TrimEnd() }
    if ($stderr) { Write-Error $stderr.TrimEnd() -ErrorAction Continue }
    if ($exitCode -notin $AcceptedExitCodes) {
        throw "O comando '$FilePath' falhou com código $exitCode."
    }
    return [pscustomobject]@{ ExitCode = $exitCode; StdOut = $stdout; StdErr = $stderr }
}

function Start-OdsNative {
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [Parameter(Mandatory)][string[]]$Arguments,
        [Parameter(Mandatory)][string]$StandardOutputPath,
        [Parameter(Mandatory)][string]$StandardErrorPath
    )

    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $FilePath
    $startInfo.WorkingDirectory = $script:RepositoryRoot
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in $Arguments) {
        [void]$startInfo.ArgumentList.Add($argument)
    }

    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    if (-not $process.Start()) { throw "Não foi possível iniciar '$FilePath'." }
    $stdoutStream = [IO.File]::Open($StandardOutputPath, 'Create', 'Write', 'Read')
    $stderrStream = [IO.File]::Open($StandardErrorPath, 'Create', 'Write', 'Read')
    return [pscustomobject]@{
        Process = $process
        StdOutCopy = $process.StandardOutput.BaseStream.CopyToAsync($stdoutStream)
        StdErrCopy = $process.StandardError.BaseStream.CopyToAsync($stderrStream)
        StdOutStream = $stdoutStream
        StdErrStream = $stderrStream
    }
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
