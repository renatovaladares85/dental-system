[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-OdsPowerShell7

$host = Get-OdsDevelopmentHost
$ports = @(8742, 8743 | ForEach-Object {
    Get-NetTCPConnection -LocalPort $_ -State Listen -ErrorAction SilentlyContinue |
        Select-Object LocalAddress, LocalPort, OwningProcess
})

[pscustomobject]@{
    state = if ($host) { $host.State } else { 'not-started' }
    pid = if ($host -and $host.Process) { $host.Process.ProcessId } else { $null }
    executable = if ($host -and $host.Process) { $host.Process.ExecutablePath } else { $null }
    commandLine = if ($host -and $host.Process) { $host.Process.CommandLine } else { $null }
    dataDirectory = $script:DevelopmentData
    health = Test-OdsHealth
    ports = $ports
    stdoutLog = Join-Path $script:LogDirectory 'web-host.stdout.log'
    stderrLog = Join-Path $script:LogDirectory 'web-host.stderr.log'
} | ConvertTo-Json -Depth 5
