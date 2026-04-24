@echo off
REM Mock XL Release CLI - 4-task pipeline with approval stage
setlocal enabledelayedexpansion

set "DEMO_DIR=%TEMP%\appcontrol-demo-rebuild"
set "LOG_FILE=%DEMO_DIR%\xlrelease.log"
set "TS=%DATE% %TIME%"

set "TEMPLATE="
set "TARGET="
:parse
if "%~1"=="" goto done_parse
if /I "%~1"=="--template" set "TEMPLATE=%~2" & shift & shift & goto parse
if /I "%~1"=="--var" (
    echo %~2 | findstr /B "target=" >nul
    if !errorlevel! equ 0 (
        for /f "tokens=1,* delims==" %%a in ("%~2") do set "TARGET=%%b"
    )
    shift & shift & goto parse
)
shift
goto parse
:done_parse

echo [!TS!] XL RELEASE CLI INVOKED >> "%LOG_FILE%"
echo   template=!TEMPLATE! target=!TARGET! >> "%LOG_FILE%"

echo.
echo [XL RELEASE] Release pipeline triggered
echo [XL RELEASE]   Template: !TEMPLATE!
echo [XL RELEASE]   Target:   !TARGET!
echo.
echo [XL RELEASE]   [Task 1/4] Pre-change approval (auto-approved for demo)...
timeout /t 2 /nobreak >nul
echo [XL RELEASE]   [Task 2/4] Backup current state...
timeout /t 1 /nobreak >nul
echo [XL RELEASE]   [Task 3/4] Executing rebuild playbook...
timeout /t 2 /nobreak >nul

echo !TARGET! | findstr /R "oracle-rac" >nul
if !errorlevel! equ 0 (
    if not exist "%DEMO_DIR%\primary" mkdir "%DEMO_DIR%\primary"
    type nul > "%DEMO_DIR%\primary\oracle-rac.running"
    echo [XL RELEASE]   Marker restored: %DEMO_DIR%\primary\oracle-rac.running
)

echo [XL RELEASE]   [Task 4/4] Post-change validation...
timeout /t 1 /nobreak >nul
echo [XL RELEASE] Release complete. All tasks successful.
echo [!TS!] Release complete >> "%LOG_FILE%"
endlocal
exit /b 0
