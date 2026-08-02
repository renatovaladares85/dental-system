@{
    IncludeRules = @(
        'PSAvoidUsingCmdletAliases',
        'PSAvoidUsingInvokeExpression',
        'PSAvoidUsingPlainTextForPassword'
    )
    Severity = @('Error', 'Warning')
}
