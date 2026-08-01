@echo off
setlocal EnableExtensions DisableDelayedExpansion
chcp 65001 >nul 2>&1

set "ODS_SCRIPT=%~dp0scripts\open-local.ps1"
if not exist "%ODS_SCRIPT%" set "ODS_SCRIPT=%ProgramFiles%\Offline Dental System\scripts\open-local.ps1"

if not exist "%ODS_SCRIPT%" (
  echo.
  echo ERRO: o Offline Dental System nao esta instalado.
  echo Execute Instalar-e-Iniciar.bat primeiro.
  echo.
  pause
  exit /b 2
)

powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%ODS_SCRIPT%"
set "ODS_EXIT_CODE=%ERRORLEVEL%"

if not "%ODS_EXIT_CODE%"=="0" (
  echo.
  echo Nao foi possivel abrir o Offline Dental System.
  echo.
  pause
)

exit /b %ODS_EXIT_CODE%
