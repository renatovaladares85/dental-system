[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot '..\dev\common.ps1')

$executable = 'C:\repo with spaces\src-tauri\target\debug\offline-dental-system.exe'
$dataDirectory = 'C:\repo with spaces\.local-data\dev-host\Data'
$valid = @($executable, '--console', '--data-directory', $dataDirectory)
if (-not (Test-OdsDevelopmentHostArguments -ActualArguments $valid -ExpectedExecutable $executable -ExpectedDataDirectory $dataDirectory)) {
    throw 'Argumentos válidos do host foram recusados.'
}

$commandLine = ConvertTo-OdsWindowsCommandLine $valid
$parsedArguments = ConvertFrom-OdsWindowsCommandLine $commandLine
if (-not (Test-OdsDevelopmentHostArguments -ActualArguments $parsedArguments -ExpectedExecutable $executable -ExpectedDataDirectory $dataDirectory)) {
    throw "Round-trip Unicode da linha de comando falhou: '$($parsedArguments -join ' | ')'."
}

foreach ($arguments in @(
    @($executable, '--console', '--data-directory', 'C:\repo with spaces\.local-data\dev-host\Data-Evil'),
    @($executable, '--console', '--data-directory', 'C:\repo with spaces\.local-data\dev-host\Data', '--extra'),
    @($executable, '--service', '--data-directory', $dataDirectory),
    @('C:\repo with spaces\other.exe', '--console', '--data-directory', $dataDirectory)
)) {
    if (Test-OdsDevelopmentHostArguments -ActualArguments $arguments -ExpectedExecutable $executable -ExpectedDataDirectory $dataDirectory) {
        throw "Argumentos divergentes foram aceitos: $($arguments -join ' ')"
    }
}

Write-Host 'SUCESSO: argumentos do PID são validados exatamente.' -ForegroundColor Green
