@echo off
setlocal EnableExtensions DisableDelayedExpansion

set "ODS_NEW_LAUNCHER=%~dp0..\Instalar-e-Iniciar.bat"
if not exist "%ODS_NEW_LAUNCHER%" (
  echo.
  echo ERRO: este launcher legado nao encontrou Instalar-e-Iniciar.bat na raiz do projeto.
  echo.
  pause
  exit /b 2
)

call "%ODS_NEW_LAUNCHER%"
exit /b %ERRORLEVEL%
