[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
. (Join-Path $repositoryRoot 'scripts\tools\wix-tooling.ps1')

$testRoot = Join-Path $repositoryRoot ('.local-data\tests\wix-process-' + [guid]::NewGuid().ToString('N'))
$logRoot = Join-Path $testRoot 'logs com acentuação'
$successScript = Join-Path $testRoot 'sucesso unicode.js'
$failureScript = Join-Path $testRoot 'falha unicode.js'
$encoding = [Text.UTF8Encoding]::new($false)
$originalTestSecret = $env:ODS_TEST_SECRET_VALUE

try {
    [void](New-Item -ItemType Directory -Path $testRoot -Force)
    [IO.File]::WriteAllText(
        $successScript,
        "process.stdout.write('✓ saída válida: sessão ação\n'); process.stderr.write('diagnóstico válido: conexão\n');",
        $encoding
    )
    [IO.File]::WriteAllText(
        $failureScript,
        "process.stdout.write('stdout preservado: confirmação\n'); process.stderr.write('stderr preservado: conexão\n'); process.exit(2);",
        $encoding
    )

    $node = (Get-Command 'node.exe' -ErrorAction Stop).Source
    $errorCountBefore = $Error.Count
    $success = Invoke-OdsWixProcess `
        -Stage 'sucesso-unicode' `
        -Executable $node `
        -Arguments @($successScript) `
        -WorkingDirectory $repositoryRoot `
        -LogRoot $logRoot `
        -ExpectedVersion 'processo auxiliar' `
        -PreserveSuccessLogs
    if ($success.ExitCode -ne 0 -or
        $success.StdOut.TrimEnd() -cne '✓ saída válida: sessão ação' -or
        $success.StdErr.TrimEnd() -cne 'diagnóstico válido: conexão' -or
        $Error.Count -ne $errorCountBefore) {
        throw 'Processo bem-sucedido não preservou UTF-8 ou criou ErrorRecord falso.'
    }
    foreach ($logPath in $success.StdOutLog, $success.StdErrLog) {
        if (-not (Test-Path -LiteralPath $logPath -PathType Leaf) -or
            -not ([IO.Path]::GetFullPath($logPath)).StartsWith([IO.Path]::GetFullPath($testRoot), [StringComparison]::OrdinalIgnoreCase)) {
            throw "Log de teste foi criado fora de .local-data: '$logPath'."
        }
    }

    $secret = 'ODS-SEGREDO-QUE-NAO-DEVE-SER-REGISTRADO'
    $env:ODS_TEST_SECRET_VALUE = $secret
    $failureMessage = $null
    try {
        $null = Invoke-OdsWixProcess `
            -Stage 'build diagnóstico' `
            -Executable $node `
            -Arguments @($failureScript, 'argumento com espaço', 'caminho com ação') `
            -WorkingDirectory $repositoryRoot `
            -LogRoot $logRoot `
            -ExpectedVersion '4.0.6'
        throw 'Processo com código 2 foi aceito indevidamente.'
    } catch {
        $failureMessage = $_.Exception.Message
    }

    foreach ($expected in @(
        "etapa 'build diagnóstico'",
        $node,
        'Comando:',
        '"argumento com espaço"',
        '"caminho com ação"',
        'Código de saída:',
        '2',
        'stdout preservado: confirmação',
        'stderr preservado: conexão',
        'Logs:'
    )) {
        if (-not $failureMessage.Contains($expected, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Diagnóstico WiX não contém '$expected'. Mensagem: $failureMessage"
        }
    }
    if ($failureMessage.Contains($secret, [StringComparison]::Ordinal)) {
        throw 'Diagnóstico registrou informação sensível não fornecida ao processo.'
    }

    $logMatches = [regex]::Matches($failureMessage, '(?m)^[A-Z]:\\.*\.(?:stdout|stderr)\.log\r?$')
    if ($logMatches.Count -ne 2) { throw 'Diagnóstico não informou os dois logs preservados.' }
    foreach ($match in $logMatches) {
        $logPath = $match.Value.Trim()
        if (-not (Test-Path -LiteralPath $logPath -PathType Leaf)) {
            throw "Log de falha não foi preservado: '$logPath'."
        }
    }
} finally {
    $env:ODS_TEST_SECRET_VALUE = $originalTestSecret
    if (Test-Path -LiteralPath $testRoot) { Remove-Item -LiteralPath $testRoot -Recurse -Force }
}

Write-Host 'SUCESSO: falhas WiX preservam contexto, Unicode e logs acionáveis.' -ForegroundColor Green
