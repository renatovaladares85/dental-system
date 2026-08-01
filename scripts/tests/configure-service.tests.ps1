[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$scriptPath = Join-Path $repositoryRoot 'installer\configure-service.ps1'
. $scriptPath -UnitTest

$installDirectory = 'C:\Program Files\Offline Dental System'
$expectedExecutable = Join-Path $installDirectory 'offline-dental-system.exe'
$valid = Get-OdsExpectedServiceImagePath -InstallDirectory $installDirectory

foreach ($actual in @(
    $valid,
    '"C:\Program Files\Offline Dental System\OFFLINE-DENTAL-SYSTEM.EXE" --service'
)) {
    if (-not (Test-OdsServiceImagePath -ActualImagePath $actual -ExpectedExecutable $expectedExecutable)) {
        throw "ImagePath válido recusado: $actual"
    }
}

foreach ($actual in @(
    'C:\Program Files\Offline Dental System\offline-dental-system.exe --service',
    '"C:\Outro Local\offline-dental-system.exe" --service',
    '"C:\Program Files\Offline Dental System\offline-dental-system.exe" --console',
    '"C:\Program Files\Offline Dental System\offline-dental-system.exe"',
    '"C:\Program Files\Offline Dental System\offline-dental-system.exe" --service --extra',
    '"C:\Program Files\Offline Dental System Evil\offline-dental-system.exe" --service',
    '"C:\Program Files\Offline Dental System\offline-dental-system.exe.bak" --service'
)) {
    if (Test-OdsServiceImagePath -ActualImagePath $actual -ExpectedExecutable $expectedExecutable) {
        throw "ImagePath inválido aceito: $actual"
    }
}

Write-Host 'SUCESSO: validação pura do ImagePath do serviço passou.' -ForegroundColor Green
