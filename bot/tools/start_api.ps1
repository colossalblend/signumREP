param([switch]$Restart)
$ErrorActionPreference = 'Stop'
$apiRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$apiLogRoot = Join-Path $apiRoot 'data\logs'
$apiPidPath = Join-Path $apiLogRoot 'api.pid'
New-Item -ItemType Directory -Path $apiLogRoot -Force | Out-Null
if (Test-Path -LiteralPath $apiPidPath) {
    $apiProcessId = [int](Get-Content -LiteralPath $apiPidPath -Raw).Trim()
    $runningApi = Get-CimInstance Win32_Process -Filter "ProcessId=$apiProcessId" -ErrorAction SilentlyContinue
    if ($runningApi -and $runningApi.CommandLine -match '-m\s+signum_bot\.api_server(?:\s|$)') {
        if (-not $Restart) {
            Write-Output "API already running, PID $apiProcessId."
            exit 0
        }
        $apiChildren = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$apiProcessId")
        foreach ($apiChild in $apiChildren) {
            if ($apiChild.Name -in @('ipatool.exe', 'zsign.exe')) {
                Stop-Process -Id $apiChild.ProcessId -Force -ErrorAction SilentlyContinue
            }
        }
        Stop-Process -Id $apiProcessId -Force
        Start-Sleep -Seconds 1
    }
}
$env:DATA_DIR = Join-Path $apiRoot 'data'
$env:PYTHONIOENCODING = 'utf-8'
$env:PYTHONUNBUFFERED = '1'
if (-not $env:IPATOOL -or -not (Test-Path -LiteralPath $env:IPATOOL)) {
    $apiToolCandidates = @(
        (Join-Path $apiRoot '..\apple-tools\windows-x64\v6\ipatool.exe'),
        (Join-Path $apiRoot '..\apple-tools\windows-x64\v5\ipatool.exe')
    )
    foreach ($apiToolCandidate in $apiToolCandidates) {
        if (Test-Path -LiteralPath $apiToolCandidate) {
            $env:IPATOOL = (Resolve-Path -LiteralPath $apiToolCandidate).Path
            break
        }
    }
}
$apiProcess = Start-Process -FilePath (Get-Command python).Source -ArgumentList '-m', 'signum_bot.api_server' `
    -WorkingDirectory $apiRoot -WindowStyle Hidden -PassThru `
    -RedirectStandardOutput (Join-Path $apiLogRoot 'api.stdout.log') `
    -RedirectStandardError (Join-Path $apiLogRoot 'api.stderr.log')
Set-Content -LiteralPath $apiPidPath -Value $apiProcess.Id -Encoding ascii
Start-Sleep -Seconds 2
if (-not (Get-Process -Id $apiProcess.Id -ErrorAction SilentlyContinue)) {
    throw 'API stopped during startup. Check data/logs/api.stderr.log.'
}
Write-Output "API started, PID $($apiProcess.Id)."
