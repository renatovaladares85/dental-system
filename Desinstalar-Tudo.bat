@echo off
setlocal EnableExtensions DisableDelayedExpansion
chcp 65001 >nul 2>&1

set "ODS_SCRIPT=%~dp0scripts\uninstall-local.ps1"
if not exist "%ODS_SCRIPT%" set "ODS_SCRIPT=%ProgramFiles%\Offline Dental System\scripts\uninstall-local.ps1"

if not exist "%ODS_SCRIPT%" (
  echo.
  echo ERRO: o desinstalador nao foi encontrado.
  echo.
  pause
  exit /b 2
)

pushd "%TEMP%"
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%ODS_SCRIPT%" -RemoveData
set "ODS_EXIT_CODE=%ERRORLEVEL%"
popd

if not "%ODS_EXIT_CODE%"=="0" (
  echo.
  echo A remocao completa nao foi realizada.
  echo.
  pause
  exit /b %ODS_EXIT_CODE%
)

echo.
echo Offline Dental System e dados locais removidos.
echo.
pause
exit /b 0
