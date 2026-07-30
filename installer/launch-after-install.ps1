$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$healthUrl = 'http://127.0.0.1:8742/api/v1/health'
$applicationUrl = 'http://127.0.0.1:8742'
$deadline = [DateTime]::UtcNow.AddSeconds(60)

while ([DateTime]::UtcNow -lt $deadline) {
    try {
        $response = Invoke-WebRequest -Uri $healthUrl -UseBasicParsing -TimeoutSec 2
        if ($response.StatusCode -eq 200) {
            Start-Process $applicationUrl
            exit 0
        }
    } catch {
        # Retry while the service finishes initialization.
    }
    Start-Sleep -Milliseconds 500
}

exit 1
