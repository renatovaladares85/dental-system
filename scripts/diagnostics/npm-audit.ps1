[CmdletBinding()]
param(
    [switch]$ShowJson
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot '..\dev\common.ps1')

Assert-OdsPowerShell7
Update-OdsProcessPath

$diagnosticsDirectory = Join-Path $script:RepositoryRoot '.local-data\diagnostics'
$outputPath = Join-Path $diagnosticsDirectory 'npm-audit.json'
[void](New-Item -ItemType Directory -Path $diagnosticsDirectory -Force)

$npm = Get-OdsNpmCommand
$result = Invoke-OdsNative `
    -FilePath $npm.NodePath `
    -AcceptedExitCodes @(0, 1) `
    -SuppressOutput `
    -Arguments @($npm.NpmCliPath, 'audit', '--json')

$auditJson = $result.StdOut.Trim()
if ([string]::IsNullOrWhiteSpace($auditJson)) {
    throw 'npm audit não produziu um relatório JSON.'
}

try {
    $audit = $auditJson | ConvertFrom-Json
} catch {
    throw 'npm audit produziu um relatório JSON inválido.'
}

$encoding = [Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText($outputPath, $auditJson, $encoding)

function Get-OdsNpmDependencyPath([string]$PackageName, [string[]]$FallbackNodes) {
    try {
        $explainResult = Invoke-OdsNative `
            -FilePath $npm.NodePath `
            -SuppressOutput `
            -Arguments @($npm.NpmCliPath, 'explain', $PackageName, '--json')
        $explained = @($explainResult.StdOut | ConvertFrom-Json)[0]
        $segments = [Collections.Generic.List[string]]::new()
        $segments.Insert(0, "$($explained.name)@$($explained.version)")
        $current = $explained
        for ($depth = 0; $depth -lt 20; $depth++) {
            $dependent = @($current.dependents)[0]
            if ($null -eq $dependent) { break }
            $parent = $dependent.from
            if (-not $parent.name) {
                $segments.Insert(0, 'root')
                break
            }
            $segments.Insert(0, "$($parent.name)@$($parent.version)")
            $current = $parent
        }
        return $segments -join ' > '
    } catch {
        return $FallbackNodes -join ', '
    }
}

$summary = foreach ($property in @($audit.vulnerabilities.PSObject.Properties)) {
    $vulnerability = $property.Value
    $advisories = @($vulnerability.via | Where-Object { $_ -isnot [string] })
    [pscustomobject]@{
        Package = $property.Name
        Severity = $vulnerability.severity
        Dependency = if ($vulnerability.isDirect) { 'direct' } else { 'transitive' }
        Range = $vulnerability.range
        FixAvailable = if ($vulnerability.fixAvailable -is [bool]) {
            $vulnerability.fixAvailable
        } else {
            "$($vulnerability.fixAvailable.name)@$($vulnerability.fixAvailable.version) (major=$($vulnerability.fixAvailable.isSemVerMajor))"
        }
        Path = Get-OdsNpmDependencyPath -PackageName $property.Name -FallbackNodes @($vulnerability.nodes)
        Advisory = @($advisories | ForEach-Object { "$($_.title) [$($_.url)]" }) -join '; '
    }
}

Write-Host "Relatório: $outputPath"
if ($summary) {
    $summary | Format-List | Out-Host
} else {
    Write-Host 'Nenhuma vulnerabilidade npm encontrada.' -ForegroundColor Green
}
if ($ShowJson) { Write-Host $auditJson }

if ($result.ExitCode -eq 1) {
    Write-Host 'npm audit encontrou vulnerabilidades; o relatório foi preservado.' -ForegroundColor Yellow
    exit 1
}

Write-Host 'SUCESSO: npm audit não encontrou vulnerabilidades.' -ForegroundColor Green
