$ErrorActionPreference = "Stop"

$env:Path = "C:\Program Files\nodejs;$env:USERPROFILE\.cargo\bin;" + $env:Path
Set-Location $PSScriptRoot

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Команда '$Name' не найдена. Проверьте PATH или запустите dev-app.cmd."
    }
}

Require-Command npm
Require-Command cargo

Write-Host "Signum dev mode (frontend + Rust, без сборки exe)" -ForegroundColor Cyan
Write-Host "Остановка: Ctrl+C" -ForegroundColor DarkGray
Write-Host ""

npm run dev:app