@echo off
setlocal EnableExtensions DisableDelayedExpansion
chcp 65001 >nul 2>&1

set "ODS_SCRIPT=%~dp0scripts\install-local.ps1"
set "ODS_DEV_ARG="

if not exist "%ODS_SCRIPT%" (
  echo.
  echo ERRO: o pacote esta incompleto. A pasta scripts deve permanecer junto deste arquivo.
  echo.
  pause
  exit /b 2
)

if exist "%~dp0.git" set "ODS_DEV_ARG=-AllowDevelopmentPackage"
if exist "%~dp0DEVELOPMENT-NOT-FOR-DISTRIBUTION.txt" set "ODS_DEV_ARG=-AllowDevelopmentPackage"

powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%ODS_SCRIPT%" %ODS_DEV_ARG%
set "ODS_EXIT_CODE=%ERRORLEVEL%"

if not "%ODS_EXIT_CODE%"=="0" (
  echo.
  echo Nao foi possivel instalar ou iniciar o Offline Dental System.
  echo Consulte a mensagem acima ou solicite suporte.
  echo.
  pause
)

exit /b %ODS_EXIT_CODE%
