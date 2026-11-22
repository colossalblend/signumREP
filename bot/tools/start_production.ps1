param([switch]$Restart, [switch]$InstallAutostart)
$ErrorActionPreference = 'Stop'
$productionRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$productionLogs = Join-Path $productionRoot 'data\logs'
New-Item -ItemType Directory -Path $productionLogs -Force | Out-Null
if ($Restart) {
    & (Join-Path $PSScriptRoot 'start_api.ps1') -Restart
    & (Join-Path $PSScriptRoot 'start_bot.ps1') -Restart
} else {
    & (Join-Path $PSScriptRoot 'start_api.ps1')
    & (Join-Path $PSScriptRoot 'start_bot.ps1')
}
$supervisorPidPath = Join-Path $productionLogs 'supervisor.pid'
$supervisorRunning = $false
if (Test-Path -LiteralPath $supervisorPidPath) {
    $supervisorProcessId = [int](Get-Content -LiteralPath $supervisorPidPath -Raw).Trim()
    $supervisorEntry = Get-CimInstance Win32_Process -Filter "ProcessId=$supervisorProcessId" -ErrorAction SilentlyContinue
    $supervisorRunning = $supervisorEntry -and $supervisorEntry.CommandLine -match '-m\s+signum_bot\.supervisor(?:\s|$)'
}
if (-not $supervisorRunning) {
    $env:DATA_DIR = Join-Path $productionRoot 'data'
    $env:PYTHONIOENCODING = 'utf-8'
    $supervisorProcess = Start-Process -FilePath (Get-Command python).Source -ArgumentList '-m', 'signum_bot.supervisor' `
        -WorkingDirectory $productionRoot -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput (Join-Path $productionLogs 'supervisor.stdout.log') `
        -RedirectStandardError (Join-Path $productionLogs 'supervisor.stderr.log')
    Set-Content -LiteralPath $supervisorPidPath -Value $supervisorProcess.Id -Encoding ascii
    Start-Sleep -Seconds 2
    if (-not (Get-Process -Id $supervisorProcess.Id -ErrorAction SilentlyContinue)) {
        throw 'Supervisor stopped during startup. Check data/logs/supervisor.stderr.log.'
    }
    Write-Output "Supervisor started, PID $($supervisorProcess.Id)."
}
if ($InstallAutostart) {
    $productionScript = Join-Path $PSScriptRoot 'start_production.ps1'
    $productionAction = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$productionScript`"" -WorkingDirectory $productionRoot
    $productionTrigger = New-ScheduledTaskTrigger -AtLogOn -User ([System.Security.Principal.WindowsIdentity]::GetCurrent().Name)
    $productionSettings = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero)
    Register-ScheduledTask -TaskName 'SignumProduction' -Action $productionAction -Trigger $productionTrigger -Settings $productionSettings -Force | Out-Null
    Write-Output 'Signum autostart registered for this Windows account.'
}
