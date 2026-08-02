[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-OdsPowerShell7

$developmentHost = Get-OdsDevelopmentHost
if ($null -eq $developmentHost) {
    Write-Host 'Nenhum host de desenvolvimento possui PID file.'
    return
}
if ($developmentHost.State -eq 'not-running') {
    Remove-OdsDevelopmentPid
    Write-Host 'O host já não estava em execução; PID file removido.'
    return
}
if ($developmentHost.State -ne 'running') {
    throw 'O PID file não corresponde ao executável e aos argumentos esperados; a parada foi recusada.'
}

Stop-Process -Id $developmentHost.Process.ProcessId -ErrorAction Stop
$process = Get-Process -Id $developmentHost.Process.ProcessId -ErrorAction SilentlyContinue
if ($process) {
    $process.WaitForExit(10000)
    if (-not $process.HasExited) { throw 'O host esperado não encerrou dentro de 10 segundos.' }
}
Remove-OdsDevelopmentPid
Write-Host 'Host de desenvolvimento encerrado e PID file removido.' -ForegroundColor Green
