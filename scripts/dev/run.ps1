[CmdletBinding()]
param(
    [switch]$OpenBrowser,
    [switch]$Detach
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-OdsPowerShell7

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
try {
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'O host de desenvolvimento não pode ser executado elevado.'
    }
} finally {
    $identity.Dispose()
}

Assert-OdsLocalPathWithoutReparsePoint $script:DevelopmentRoot
$executable = Join-Path $script:RepositoryRoot 'src-tauri\target\debug\offline-dental-system.exe'
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw 'Execute scripts/dev/build.ps1 antes de iniciar o host.' }

New-Item -ItemType Directory -Force -Path $script:DevelopmentData, $script:LogDirectory | Out-Null
if (Test-Path -LiteralPath $script:DevelopmentPidFile) {
    $existing = Get-OdsDevelopmentHost
    if ($existing.State -eq 'running') {
        throw "O host de desenvolvimento já está em execução no PID $($existing.Process.ProcessId). Use scripts/dev/status.ps1 ou scripts/dev/stop.ps1."
    }
    if ($existing.State -eq 'divergent') {
        throw 'O PID file aponta para um processo divergente; a execução foi recusada para preservar o processo.'
    }
    Remove-OdsDevelopmentPid
}
Assert-OdsPortsAvailable
$stdout = Join-Path $script:LogDirectory 'web-host.stdout.log'
$stderr = Join-Path $script:LogDirectory 'web-host.stderr.log'
$hostProcess = Start-OdsDevelopmentHost `
    -FilePath $executable `
    -Arguments @('--console', '--data-directory', $script:DevelopmentData) `
    -StandardOutputPath $stdout `
    -StandardErrorPath $stderr
Write-OdsDevelopmentPid -Process $hostProcess -Executable $executable -Arguments @('--console', '--data-directory', $script:DevelopmentData)
$deadline = [DateTime]::UtcNow.AddSeconds(45)
while ([DateTime]::UtcNow -lt $deadline -and -not $hostProcess.HasExited) {
    if (Test-OdsHealth) {
        Write-Host "SUCESSO: sistema disponível em $script:ApplicationUrl" -ForegroundColor Green
        if ($OpenBrowser) { Start-Process $script:ApplicationUrl }
        if ($Detach) {
            $hostProcess.Dispose()
            return
        }
        $exitCode = $null
        try {
            $hostProcess.WaitForExit()
        } finally {
            if (-not $hostProcess.HasExited) {
                [void]$hostProcess.WaitForExit(10000)
            }
            if ($hostProcess.HasExited) {
                $exitCode = $hostProcess.ExitCode
                Remove-OdsDevelopmentPid
            }
            $hostProcess.Dispose()
        }
        if ($null -eq $exitCode) {
            throw "O host não encerrou após a interrupção. Consulte '$stdout' e '$stderr'."
        }
        if ($exitCode -ne 0) {
            throw "O host encerrou inesperadamente com código $exitCode. Consulte '$stdout' e '$stderr'."
        }
        return
    }
    Start-Sleep -Milliseconds 500
}
if (-not $hostProcess.HasExited) {
    Stop-Process -Id $hostProcess.Id -ErrorAction SilentlyContinue
    $hostProcess.WaitForExit()
}
if ($hostProcess.HasExited) { Remove-OdsDevelopmentPid }
$hostProcess.Dispose()
throw "O host não respondeu ao health. Logs preservados: '$stdout' e '$stderr'."
