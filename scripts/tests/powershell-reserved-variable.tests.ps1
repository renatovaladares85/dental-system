[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$reserved = @('Host', 'PID', 'Error', 'HOME', 'ExecutionContext', 'PSVersionTable', 'PSScriptRoot', 'MyInvocation')
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$violations = [Collections.Generic.List[object]]::new()
$files = Get-ChildItem -Path (Join-Path $repositoryRoot 'scripts'), (Join-Path $repositoryRoot 'installer') -Recurse -File |
    Where-Object { $_.Extension -in '.ps1', '.psm1' }

foreach ($file in $files) {
    $tokens = $null
    $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile($file.FullName, [ref]$tokens, [ref]$errors)
    if ($errors.Count) { throw "Script inválido: $($file.FullName)" }

    $assignments = $ast.FindAll({ param($node) $node -is [Management.Automation.Language.AssignmentStatementAst] }, $true)
    foreach ($assignment in $assignments) {
        if ($assignment.Left -isnot [Management.Automation.Language.VariableExpressionAst]) { continue }
        $variable = $assignment.Left.VariablePath.UserPath
        if ($reserved -contains $variable) {
            $violations.Add([pscustomobject]@{
                File = $assignment.Extent.File
                Line = $assignment.Extent.StartLineNumber
                Kind = 'assignment'
                Variable = $variable
            })
        }
    }

    $parameters = $ast.FindAll({ param($node) $node -is [Management.Automation.Language.ParameterAst] }, $true)
    foreach ($parameter in $parameters) {
        $variable = $parameter.Name.VariablePath.UserPath
        if ($reserved -contains $variable) {
            $violations.Add([pscustomobject]@{
                File = $parameter.Extent.File
                Line = $parameter.Extent.StartLineNumber
                Kind = 'parameter'
                Variable = $variable
            })
        }
    }

    $foreachStatements = $ast.FindAll({ param($node) $node -is [Management.Automation.Language.ForEachStatementAst] }, $true)
    foreach ($foreachStatement in $foreachStatements) {
        $variable = $foreachStatement.Variable.VariablePath.UserPath
        if ($reserved -contains $variable) {
            $violations.Add([pscustomobject]@{
                File = $foreachStatement.Extent.File
                Line = $foreachStatement.Extent.StartLineNumber
                Kind = 'foreach'
                Variable = $variable
            })
        }
    }
}

if ($violations.Count) {
    $details = $violations | ForEach-Object { "$($_.File):$($_.Line) [$($_.Kind)] `$$($_.Variable)" }
    throw "Uso gravável de variáveis reservadas: $($details -join '; ')"
}
Write-Host 'SUCESSO: nenhuma variável reservada é sobrescrita.' -ForegroundColor Green
