@echo off
setlocal

set "VSDEVCMD=%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat"
set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
set "PORTABLE_PERL=%LOCALAPPDATA%\Programs\StrawberryPerlPortable-5.42.2.1\perl\bin"
set "PORTABLE_TOOLS=%LOCALAPPDATA%\Programs\StrawberryPerlPortable-5.42.2.1\c\bin"

if not exist "%VSDEVCMD%" (
  echo Visual Studio 2022 Build Tools with the C++ workload was not found. 1>&2
  exit /b 2
)

if not exist "%CARGO%" (
  echo Rust Cargo was not found in the default rustup location. 1>&2
  exit /b 3
)

call "%VSDEVCMD%" -no_logo -arch=x64 -host_arch=x64
if errorlevel 1 exit /b %errorlevel%

if exist "%PORTABLE_PERL%\perl.exe" (
  set "PATH=%PORTABLE_PERL%;%PATH%"
) else (
  where perl.exe >nul 2>nul
  if errorlevel 1 if exist "%ProgramFiles%\Git\usr\bin\perl.exe" (
    set "PATH=%ProgramFiles%\Git\usr\bin;%PATH%"
  )
)
if exist "%PORTABLE_TOOLS%\nasm.exe" set "PATH=%PATH%;%PORTABLE_TOOLS%"

"%CARGO%" %*
