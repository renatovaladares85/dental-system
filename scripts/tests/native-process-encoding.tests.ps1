[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot '..\dev\common.ps1')

Assert-OdsPowerShell7
Update-OdsProcessPath

$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ('ods-native-' + [guid]::NewGuid().ToString('N'))
$temporaryScript = Join-Path $temporaryDirectory 'unicode-output.js'
$encoding = [Text.UTF8Encoding]::new($false)

try {
    [void](New-Item -ItemType Directory -Path $temporaryDirectory)
    [IO.File]::WriteAllText(
        $temporaryScript,
        "process.stdout.write('✓ sessão ação confirmação\n'); process.stderr.write('diagnóstico válido: conexão\n');",
        $encoding
    )

    $errorCountBefore = $Error.Count
    $result = Invoke-OdsNative 'node.exe' $temporaryScript

    if ($result.ExitCode -ne 0) { throw "Processo Node retornou código $($result.ExitCode)." }
    if ($result.StdOut.TrimEnd() -cne '✓ sessão ação confirmação') {
        throw "stdout Unicode foi alterado: '$($result.StdOut.TrimEnd())'."
    }
    if ($result.StdErr.TrimEnd() -cne 'diagnóstico válido: conexão') {
        throw "stderr Unicode foi alterado: '$($result.StdErr.TrimEnd())'."
    }
    if ($Error.Count -ne $errorCountBefore) {
        throw 'stderr de processo bem-sucedido adicionou um ErrorRecord ao PowerShell.'
    }
} finally {
    if (Test-Path -LiteralPath $temporaryDirectory) {
        Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force
    }
}

Write-Host 'SUCESSO: stdout e stderr nativos preservam UTF-8 sem ErrorRecord falso.' -ForegroundColor Green
