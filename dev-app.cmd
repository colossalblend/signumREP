@echo off
setlocal
cd /d "%~dp0"

set "PATH=C:\Program Files\nodejs;%USERPROFILE%\.cargo\bin;%PATH%"

where npm >nul 2>&1
if errorlevel 1 (
  echo [ERROR] npm не найден. Установите Node.js: https://nodejs.org/
  pause
  exit /b 1
)

where cargo >nul 2>&1
if errorlevel 1 (
  echo [ERROR] cargo не найден. Установите Rust: https://rustup.rs/
  pause
  exit /b 1
)

tasklist /FI "IMAGENAME eq signum.exe" 2>nul | find /I "signum.exe" >nul
if not errorlevel 1 (
  echo [INFO] Signum uzhe zapuschen. Smotrite okno na paneli zadach.
  echo Esli nuzhen perezapusk: zakroyte okno Signum i nazhmite Ctrl+C v starom terminale.
  echo.
  pause
  exit /b 0
)

echo Signum dev mode - frontend + Rust, bez sborki exe
echo Ostanovka: Ctrl+C
echo.

call npm run dev:app
if errorlevel 1 (
  echo.
  echo [ERROR] dev:app zavershilsya s oshibkoy.
  pause
)