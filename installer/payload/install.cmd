@echo off
setlocal

set "APP_NAME=Signum"
set "TARGET=%LOCALAPPDATA%\Programs\Signum"
set "ZIP=%~dp0SignumPayload.zip"

if not exist "%TARGET%" mkdir "%TARGET%"

powershell -NoProfile -ExecutionPolicy Bypass -Command "Expand-Archive -LiteralPath '%ZIP%' -DestinationPath '%TARGET%' -Force"

powershell -NoProfile -ExecutionPolicy Bypass -Command "$target='%TARGET%\Signum.exe'; $work='%TARGET%'; $shortcut=(New-Object -ComObject WScript.Shell).CreateShortcut([Environment]::GetFolderPath('Programs') + '\Signum.lnk'); $shortcut.TargetPath=$target; $shortcut.WorkingDirectory=$work; $shortcut.IconLocation=$target; $shortcut.Save()"

powershell -NoProfile -ExecutionPolicy Bypass -Command "$target='%TARGET%\Signum.exe'; $work='%TARGET%'; $shortcut=(New-Object -ComObject WScript.Shell).CreateShortcut([Environment]::GetFolderPath('Desktop') + '\Signum.lnk'); $shortcut.TargetPath=$target; $shortcut.WorkingDirectory=$work; $shortcut.IconLocation=$target; $shortcut.Save()"

start "" "%TARGET%\Signum.exe"
endlocal
