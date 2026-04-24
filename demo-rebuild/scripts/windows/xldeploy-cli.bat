@echo off
REM Mock XL Deploy CLI - logs the invocation, sleeps, restores target marker
setlocal enabledelayedexpansion

set "DEMO_DIR=%TEMP%\appcontrol-demo-rebuild"
set "LOG_FILE=%DEMO_DIR%\xldeploy.log"
set "TS=%DATE% %TIME%"

echo [!TS!] XL DEPLOY CLI INVOKED >> "%LOG_FILE%"
echo   args: %* >> "%LOG_FILE%"

set "PKG="
set "TARGET="
:parse
if "%~1"=="" goto done_parse
if /I "%~1"=="--package" set "PKG=%~2" & shift & shift & goto parse
if /I "%~1"=="--target"  set "TARGET=%~2" & shift & shift & goto parse
shift
goto parse
:done_parse

echo [!TS!]   package=!PKG! target=!TARGET! >> "%LOG_FILE%"

echo.
echo [XL DEPLOY] Deployment triggered
echo [XL DEPLOY]   Package: !PKG!
echo [XL DEPLOY]   Target:  !TARGET!
echo [XL DEPLOY]   Phase 1/3: Pre-deploy checks...
timeout /t 1 /nobreak >nul
echo [XL DEPLOY]   Phase 2/3: Copying artifacts to target...
timeout /t 1 /nobreak >nul
echo [XL DEPLOY]   Phase 3/3: Activating new version...
timeout /t 1 /nobreak >nul

if not "!TARGET!"=="" (
    echo !TARGET! | findstr /R "jboss-prd-" >nul
    if !errorlevel! equ 0 (
        for /f "tokens=1,* delims=." %%a in ("!TARGET!") do set "HOSTPART=%%a"
        for /f "tokens=3 delims=-" %%a in ("!HOSTPART!") do set "NODE=%%a"
        if not "!NODE!"=="" (
            if not exist "%DEMO_DIR%\primary" mkdir "%DEMO_DIR%\primary"
            type nul > "%DEMO_DIR%\primary\jboss-!NODE!.running"
            echo [XL DEPLOY]   Marker restored: %DEMO_DIR%\primary\jboss-!NODE!.running
        )
    )
    echo !TARGET! | findstr /R "mq-prd" >nul
    if !errorlevel! equ 0 (
        if not exist "%DEMO_DIR%\primary" mkdir "%DEMO_DIR%\primary"
        type nul > "%DEMO_DIR%\primary\mq-series.running"
        echo [XL DEPLOY]   Marker restored: %DEMO_DIR%\primary\mq-series.running
    )
    echo !TARGET! | findstr /R "wsp-prd" >nul
    if !errorlevel! equ 0 (
        if not exist "%DEMO_DIR%\primary" mkdir "%DEMO_DIR%\primary"
        type nul > "%DEMO_DIR%\primary\websphere-portal.running"
        echo [XL DEPLOY]   Marker restored: %DEMO_DIR%\primary\websphere-portal.running
    )
)

echo [XL DEPLOY] Deployment complete. Target is healthy.
echo [!TS!] Deployment complete >> "%LOG_FILE%"
endlocal
exit /b 0
