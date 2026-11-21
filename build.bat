@echo off
REM ==== Inzel (Rust) build script ====
REM Step 1: cargo build --release  -> target\release\Inzel.exe
REM Step 2: if Inno Setup 6 or 7 is installed -> installer\Inzel-Setup-1.0.0.exe

cargo build --release || goto :err
echo.
echo [1/2] App built: target\release\Inzel.exe

set ISCC=
if exist "%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe" set ISCC=%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe
if exist "%ProgramFiles%\Inno Setup 6\ISCC.exe" set ISCC=%ProgramFiles%\Inno Setup 6\ISCC.exe
if exist "%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" set ISCC=%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe
if exist "%ProgramFiles(x86)%\Inno Setup 7\ISCC.exe" set ISCC=%ProgramFiles(x86)%\Inno Setup 7\ISCC.exe
if exist "%ProgramFiles%\Inno Setup 7\ISCC.exe" set ISCC=%ProgramFiles%\Inno Setup 7\ISCC.exe
if exist "%LOCALAPPDATA%\Programs\Inno Setup 7\ISCC.exe" set ISCC=%LOCALAPPDATA%\Programs\Inno Setup 7\ISCC.exe

if "%ISCC%"=="" (
  echo [2/2] Inno Setup not found - skipping installer. Install it from https://jrsoftware.org/isdl.php
  exit /b 0
)

"%ISCC%" installer.iss || goto :err
echo [2/2] Installer built: installer\Inzel-Setup-1.0.0.exe
exit /b 0

:err
echo Build failed.
exit /b 1
