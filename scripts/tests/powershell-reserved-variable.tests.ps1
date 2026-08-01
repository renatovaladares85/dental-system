[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$reserved = @('Host', 'PID', 'Error', 'HOME', 'ExecutionContext', 'PSVersionTable', 'PSScriptRoot', 'MyInvocation')
$violations = @()
Get-ChildItem -Path (Join-Path $PSScriptRoot '..') -Recurse -File -Filter '*.ps1' | ForEach-Object {
    $tokens = $null
    $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile($_.FullName, [ref]$tokens, [ref]$errors)
    if ($errors.Count) { throw "Script inválido: $($_.FullName)" }
    $assignments = $ast.FindAll({ param($node) $node -is [Management.Automation.Language.AssignmentStatementAst] }, $true)
    foreach ($assignment in $assignments) {
        if ($assignment.Left -isnot [Management.Automation.Language.VariableExpressionAst]) { continue }
        $variable = $assignment.Left.VariablePath.UserPath
        if ($reserved -contains $variable) { $violations += "$($assignment.Extent.File):$($assignment.Extent.StartLineNumber) `$${variable}" }
    }
}
if ($violations.Count) { throw "Atribuições a variáveis reservadas: $($violations -join '; ')" }
Write-Host 'SUCESSO: nenhuma variável reservada é sobrescrita.' -ForegroundColor Green
