[CmdletBinding()]
param([switch]$OpenBrowser)

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
Assert-OdsPortsAvailable
$executable = Join-Path $script:RepositoryRoot 'src-tauri\target\debug\offline-dental-system.exe'
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw 'Execute scripts/dev/build.ps1 antes de iniciar o host.' }

New-Item -ItemType Directory -Force -Path $script:DevelopmentData, $script:LogDirectory | Out-Null
$stdout = Join-Path $script:LogDirectory 'web-host.stdout.log'
$stderr = Join-Path $script:LogDirectory 'web-host.stderr.log'
$hostProcess = Start-OdsNative `
    -FilePath $executable `
    -Arguments @('--console', '--data-directory', $script:DevelopmentData) `
    -StandardOutputPath $stdout `
    -StandardErrorPath $stderr
$deadline = [DateTime]::UtcNow.AddSeconds(45)
while ([DateTime]::UtcNow -lt $deadline -and -not $hostProcess.Process.HasExited) {
    if (Test-OdsHealth) {
        Write-Host "SUCESSO: sistema disponível em $script:ApplicationUrl" -ForegroundColor Green
        if ($OpenBrowser) { Start-Process $script:ApplicationUrl }
        exit 0
    }
    Start-Sleep -Milliseconds 500
}
if (-not $hostProcess.Process.HasExited) {
    Stop-Process -Id $hostProcess.Process.Id -ErrorAction SilentlyContinue
    $hostProcess.Process.WaitForExit()
}
$hostProcess.StdOutCopy.GetAwaiter().GetResult()
$hostProcess.StdErrCopy.GetAwaiter().GetResult()
$hostProcess.StdOutStream.Dispose()
$hostProcess.StdErrStream.Dispose()
$hostProcess.Process.Dispose()
throw "O host não respondeu ao health. Logs preservados: '$stdout' e '$stderr'."
