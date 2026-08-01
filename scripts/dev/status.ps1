[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
Assert-OdsPowerShell7

$developmentHost = Get-OdsDevelopmentHost
$ports = @(8742, 8743 | ForEach-Object {
    Get-NetTCPConnection -LocalPort $_ -State Listen -ErrorAction SilentlyContinue |
        Select-Object LocalAddress, LocalPort, OwningProcess
})

[pscustomobject]@{
    state = if ($developmentHost) { $developmentHost.State } else { 'not-started' }
    managedByPidFile = $null -ne $developmentHost
    managedPid = if ($developmentHost -and $developmentHost.Process) { $developmentHost.Process.ProcessId } else { $null }
    pid = if ($developmentHost -and $developmentHost.Process) { $developmentHost.Process.ProcessId } else { $null }
    executable = if ($developmentHost -and $developmentHost.Process) { $developmentHost.Process.ExecutablePath } else { $null }
    commandLine = if ($developmentHost -and $developmentHost.Process) { $developmentHost.Process.CommandLine } else { $null }
    dataDirectory = $script:DevelopmentData
    health = Test-OdsHealth
    listeners = @($ports | ForEach-Object {
        [pscustomobject]@{
            port = $_.LocalPort
            pid = $_.OwningProcess
            managed = $developmentHost -and $developmentHost.Process -and $_.OwningProcess -eq $developmentHost.Process.ProcessId
            executable = if ($developmentHost -and $developmentHost.Process -and $_.OwningProcess -eq $developmentHost.Process.ProcessId) { $developmentHost.Process.ExecutablePath } else { $null }
            commandLineMatchesExpectedHost = $developmentHost -and $developmentHost.State -eq 'running' -and $_.OwningProcess -eq $developmentHost.Process.ProcessId
        }
    })
    stdoutLog = Join-Path $script:LogDirectory 'web-host.stdout.log'
    stderrLog = Join-Path $script:LogDirectory 'web-host.stderr.log'
} | ConvertTo-Json -Depth 5
