param([switch]$Restart)

$ErrorActionPreference = 'Stop'
$botRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$logRoot = Join-Path $botRoot 'data\logs'
$pidPath = Join-Path $logRoot 'bot.pid'
New-Item -ItemType Directory -Path $logRoot -Force | Out-Null

if (Test-Path -LiteralPath $pidPath) {
    $botProcessId = [int](Get-Content -LiteralPath $pidPath -Raw).Trim()
    $runningBot = Get-CimInstance Win32_Process -Filter "ProcessId=$botProcessId" -ErrorAction SilentlyContinue
    if ($runningBot -and $runningBot.CommandLine -match '-m\s+signum_bot(?:\s|$)') {
        if (-not $Restart) {
            Write-Output "Bot already running, PID $botProcessId. Use -Restart to restart."
            exit 0
        }
        $childProcesses = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$botProcessId")
        foreach ($child in $childProcesses) {
            if ($child.Name -in @('ipatool.exe', 'zsign.exe', 'signum-apple-bridge.exe')) {
                Stop-Process -Id $child.ProcessId -Force -ErrorAction SilentlyContinue
            }
        }
        Stop-Process -Id $botProcessId -Force
        Start-Sleep -Seconds 1
    }
}

$env:DATA_DIR = Join-Path $botRoot 'data'
$env:PYTHONIOENCODING = 'utf-8'
$env:PYTHONUNBUFFERED = '1'
if (-not $env:IPATOOL -or -not (Test-Path -LiteralPath $env:IPATOOL)) {
    $candidatePaths = @(
        (Join-Path $botRoot '..\apple-tools\windows-x64\v6\ipatool.exe'),
        (Join-Path $botRoot '..\apple-tools\windows-x64\v5\ipatool.exe')
    )
    foreach ($candidate in $candidatePaths) {
        if (Test-Path -LiteralPath $candidate) {
            $env:IPATOOL = (Resolve-Path -LiteralPath $candidate).Path
            break
        }
    }
}

$botProcess = Start-Process -FilePath (Get-Command python).Source -ArgumentList '-m', 'signum_bot' `
    -WorkingDirectory $botRoot -WindowStyle Hidden -PassThru `
    -RedirectStandardOutput (Join-Path $logRoot 'bot.stdout.log') `
    -RedirectStandardError (Join-Path $logRoot 'bot.stderr.log')
Set-Content -LiteralPath $pidPath -Value $botProcess.Id -Encoding ascii
Start-Sleep -Seconds 2
if (-not (Get-Process -Id $botProcess.Id -ErrorAction SilentlyContinue)) {
    throw 'Bot stopped during startup. Check data/logs/bot.stderr.log.'
}
Write-Output "Bot started, PID $($botProcess.Id)."
