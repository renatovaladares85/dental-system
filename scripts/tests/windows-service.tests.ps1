[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$modulePath = Join-Path (Split-Path -Parent $PSScriptRoot) 'lib\windows-service.psm1'
Import-Module -Name $modulePath -Force

$script:failures = [Collections.Generic.List[string]]::new()
$script:passed = 0

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function Assert-Equal($Expected, $Actual, [string]$Message) {
    if ($Expected -is [array] -or $Actual -is [array]) {
        $expectedJson = @($Expected) | ConvertTo-Json -Compress
        $actualJson = @($Actual) | ConvertTo-Json -Compress
        if ($expectedJson -cne $actualJson) { throw "$Message Esperado=$expectedJson Atual=$actualJson" }
    } elseif ($Expected -cne $Actual) {
        throw "$Message Esperado='$Expected' Atual='$Actual'"
    }
}

function Assert-Throws([scriptblock]$Action, [string]$Pattern, [string]$Message) {
    try { & $Action } catch {
        if ($_.Exception.Message -notmatch $Pattern) {
            throw "$Message Exceção inesperada: $($_.Exception.Message)"
        }
        return
    }
    throw "$Message Nenhuma exceção foi lançada."
}

function Test-Case([string]$Name, [scriptblock]$Action) {
    try {
        & $Action
        $script:passed++
        Write-Host "OK  $Name" -ForegroundColor Green
    } catch {
        $script:failures.Add("$Name`: $($_.Exception.Message)")
        Write-Host "FALHA  $Name`: $($_.Exception.Message)" -ForegroundColor Red
    }
}

function New-TestServiceState([bool]$Exists, [string]$ImagePath, [string]$Executable) {
    if ($Executable) {
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Executable) | Out-Null
        [IO.File]::WriteAllBytes($Executable, [byte[]](1))
    }
    return [pscustomobject]@{
        Exists = $Exists
        Name = 'OfflineDentalSystem'
        Status = if ($Exists) { 'Stopped' } else { 'Absent' }
        ImagePath = $ImagePath
        ObjectName = if ($Exists) { 'NT AUTHORITY\LocalService' } else { $null }
        DisplayName = if ($Exists) { 'Offline Dental System' } else { $null }
        Start = if ($Exists) { 2 } else { 0 }
        DelayedAutoStart = if ($Exists) { 1 } else { 0 }
        ServiceSidType = if ($Exists) { 3 } else { 0 }
    }
}

function New-FakeServiceHarness($State) {
    $calls = [Collections.Generic.List[object]]::new()
    $invoker = {
        param($Arguments, $AcceptedCodes, $Metadata)
        $argumentsCopy = @($Arguments | ForEach-Object { [string]$_ })
        $calls.Add($argumentsCopy)
        $command = $argumentsCopy[0]
        if ($command -eq 'create') {
            $State.Exists = $true
            $State.Status = 'Stopped'
        }
        if ($command -in @('create', 'config')) {
            for ($index = 2; $index -lt ($argumentsCopy.Count - 1); $index += 2) {
                switch ($argumentsCopy[$index]) {
                    'binPath=' { $State.ImagePath = $argumentsCopy[$index + 1] }
                    'obj=' { $State.ObjectName = $argumentsCopy[$index + 1] }
                    'DisplayName=' { $State.DisplayName = $argumentsCopy[$index + 1] }
                    'start=' {
                        $State.Start = 2
                        $State.DelayedAutoStart = if ($argumentsCopy[$index + 1] -eq 'delayed-auto') { 1 } else { 0 }
                    }
                }
            }
        }
        if ($command -eq 'sidtype') { $State.ServiceSidType = 3 }
        if ($command -eq 'stop') { $State.Status = 'Stopped' }
        if ($command -eq 'start') { $State.Status = 'Running' }
        if ($command -eq 'delete') { $State.Exists = $false; $State.Status = 'Absent' }
        return [pscustomobject]@{ ExitCode = 0 }
    }.GetNewClosure()
    $provider = { return $State }.GetNewClosure()
    return [pscustomobject]@{ Calls = $calls; Invoker = $invoker; Provider = $provider }
}

$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ('ods-service-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporaryRoot | Out-Null
try {
    Test-Case 'preserva argumentos nativos com espaços, aspas e --service' {
        $probeDirectory = Join-Path $temporaryRoot 'probe com espaço'
        New-Item -ItemType Directory -Path $probeDirectory | Out-Null
        $probe = Join-Path $probeDirectory 'argument probe.exe'
        $source = @'
using System;
using System.Text;
public static class ArgumentProbe {
    public static int Main(string[] args) {
        foreach (var value in args) {
            Console.WriteLine(Convert.ToBase64String(Encoding.UTF8.GetBytes(value)));
        }
        return 0;
    }
}
'@
        Add-Type -TypeDefinition $source -Language CSharp -OutputAssembly $probe -OutputType ConsoleApplication
        $expected = @(
            'create',
            'OfflineDentalSystem',
            'binPath=',
            '"C:\Program Files\Offline Dental System\versions\0.1.0\offline-dental-system.exe" --service',
            'obj=',
            'NT AUTHORITY\LocalService',
            'DisplayName=',
            'Offline Dental System'
        )
        $result = Invoke-OdsNativeProcess -FilePath $probe -Arguments $expected
        $actual = @($result.StandardOutput -split "`r?`n" | Where-Object { $_ } | ForEach-Object {
            [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($_))
        })
        Assert-Equal $expected $actual 'Os argumentos recebidos pelo processo filho divergiram.'
    }

    Test-Case 'cria serviço com argumentos separados e configuração segura' {
        $installRoot = Join-Path $temporaryRoot 'produto novo'
        $executable = Join-Path $installRoot 'versions\0.1.0\offline-dental-system.exe'
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $executable) | Out-Null
        [IO.File]::WriteAllBytes($executable, [byte[]](1))
        $state = New-TestServiceState $false $null $null
        $harness = New-FakeServiceHarness $state
        $created = $false
        $changed = $false
        Set-OdsWindowsService -ServiceName 'OfflineDentalSystem' -ProductName 'Offline Dental System' `
            -Executable $executable -InstallRoot $installRoot -CreatedThisRun ([ref]$created) `
            -ChangedThisRun ([ref]$changed) -NativeInvoker $harness.Invoker -SnapshotProvider $harness.Provider | Out-Null
        Assert-True $created 'O serviço criado não foi marcado como recurso desta execução.'
        $create = @($harness.Calls | Where-Object { $_[0] -eq 'create' })[0]
        Assert-Equal 'binPath=' $create[2] 'binPath deve ser um argumento independente.'
        Assert-Equal ('"' + $executable + '" --service') $create[3] 'O ImagePath deve chegar como um argumento único.'
        Assert-Equal 'start=' $create[4] 'start= deve ser independente.'
        Assert-Equal 'auto' $create[5] 'O valor de start deve ser independente.'
        Assert-True (@($harness.Calls | Where-Object { $_[0] -eq 'sidtype' }).Count -eq 1) 'sidtype restricted não foi configurado.'
        Assert-True (@($harness.Calls | Where-Object { $_[0] -eq 'failure' }).Count -eq 1) 'A política de recuperação não foi configurada.'
    }

    Test-Case 'falha de create mantém marcador falso e rollback não apaga serviço alheio' {
        $state = New-TestServiceState $false $null $null
        $calls = [Collections.Generic.List[object]]::new()
        $failing = { param($Arguments, $AcceptedCodes, $Metadata) $calls.Add(@($Arguments)); throw 'código 1639' }.GetNewClosure()
        $provider = { return $state }.GetNewClosure()
        $created = $false
        $changed = $false
        Assert-Throws {
            Set-OdsWindowsService -ServiceName 'OfflineDentalSystem' -ProductName 'Offline Dental System' `
                -Executable (Join-Path $temporaryRoot 'never.exe') -InstallRoot (Join-Path $temporaryRoot 'empty') `
                -CreatedThisRun ([ref]$created) -ChangedThisRun ([ref]$changed) `
                -NativeInvoker $failing -SnapshotProvider $provider
        } '1639' 'A falha do sc.exe deveria ser propagada.'
        Assert-True (-not $created) 'Falha de criação não pode marcar o serviço como criado.'
        $rollbackCalls = [Collections.Generic.List[object]]::new()
        $rollbackInvoker = { param($Arguments, $AcceptedCodes, $Metadata) $rollbackCalls.Add(@($Arguments)) }.GetNewClosure()
        Undo-OdsWindowsServiceChange -ServiceName 'OfflineDentalSystem' -CreatedThisRun $false `
            -ChangedThisRun $false -PreviousSnapshot $state -NativeInvoker $rollbackInvoker -SnapshotProvider $provider
        Assert-Equal 0 $rollbackCalls.Count 'Rollback sem recurso criado não deve chamar stop ou delete.'
    }

    Test-Case 'reexecução atualiza serviço compatível sem recriá-lo' {
        $installRoot = Join-Path $temporaryRoot 'produto existente'
        $oldExecutable = Join-Path $installRoot 'versions\0.0.9\offline-dental-system.exe'
        $newExecutable = Join-Path $installRoot 'versions\0.1.0\offline-dental-system.exe'
        $state = New-TestServiceState $true ('"' + $oldExecutable + '" --service') $oldExecutable
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $newExecutable) | Out-Null
        [IO.File]::WriteAllBytes($newExecutable, [byte[]](1))
        $harness = New-FakeServiceHarness $state
        $created = $false
        $changed = $false
        Set-OdsWindowsService -ServiceName 'OfflineDentalSystem' -ProductName 'Offline Dental System' `
            -Executable $newExecutable -InstallRoot $installRoot -CreatedThisRun ([ref]$created) `
            -ChangedThisRun ([ref]$changed) -NativeInvoker $harness.Invoker -SnapshotProvider $harness.Provider | Out-Null
        Assert-True (-not $created) 'Serviço preexistente não pode ser tratado como recém-criado.'
        Assert-True $changed 'Serviço preexistente atualizado deve ser marcado como alterado.'
        Assert-Equal 0 @($harness.Calls | Where-Object { $_[0] -eq 'create' }).Count 'Reexecução não deve recriar o serviço.'
        Assert-Equal 1 @($harness.Calls | Where-Object { $_[0] -eq 'config' -and $_ -contains 'binPath=' }).Count 'Reexecução deve atualizar o ImagePath uma vez.'
    }

    Test-Case 'serviço conflitante falha antes de qualquer alteração' {
        $state = New-TestServiceState $true '"C:\Outro Produto\server.exe" --service' $null
        $state.ObjectName = 'LocalSystem'
        $harness = New-FakeServiceHarness $state
        $created = $false
        $changed = $false
        Assert-Throws {
            Set-OdsWindowsService -ServiceName 'OfflineDentalSystem' -ProductName 'Offline Dental System' `
                -Executable (Join-Path $temporaryRoot 'new.exe') -InstallRoot (Join-Path $temporaryRoot 'controlled') `
                -CreatedThisRun ([ref]$created) -ChangedThisRun ([ref]$changed) `
                -NativeInvoker $harness.Invoker -SnapshotProvider $harness.Provider
        } 'incompatível' 'Serviço conflitante deveria ser recusado.'
        Assert-Equal 0 $harness.Calls.Count 'Conflito deve falhar antes de chamar sc.exe.'
    }

    Test-Case 'mensagens UTF-8 sobrevivem ao processo filho e ao JSONL' {
        $child = Join-Path $temporaryRoot 'filho utf8.ps1'
        $childLog = Join-Path $temporaryRoot 'log utf8.jsonl'
        $childSource = @'
param([string]$Message, [string]$LogPath)
$record = [ordered]@{ message = $Message; ação = 'instalação concluída' }
[IO.File]::AppendAllText($LogPath, (($record | ConvertTo-Json -Compress) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
'@
        [IO.File]::WriteAllText($child, $childSource, [Text.UTF8Encoding]::new($true))
        $message = 'serviço: operação concluída; código válido'
        $process = Start-OdsProcess -FilePath 'powershell.exe' -Arguments @(
            '-NoLogo', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $child,
            '-Message', $message, '-LogPath', $childLog
        ) -Wait -PassThru
        Assert-Equal 0 $process.ExitCode 'O processo filho UTF-8 falhou.'
        $record = Get-Content -Raw -Encoding UTF8 -LiteralPath $childLog | ConvertFrom-Json
        Assert-Equal $message ([string]$record.message) 'A mensagem acentuada foi corrompida.'
        Assert-Equal 'instalação concluída' ([string]$record.ação) 'O JSONL UTF-8 foi corrompido.'
    }
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) { Remove-Item -LiteralPath $temporaryRoot -Recurse -Force }
}

if ($script:failures.Count -gt 0) {
    Write-Host ''
    $script:failures | ForEach-Object { Write-Host $_ -ForegroundColor Red }
    throw "$($script:failures.Count) teste(s) de serviço Windows falharam; $script:passed passaram."
}
Write-Host "SUCESSO: $script:passed testes de serviço Windows passaram." -ForegroundColor Green
