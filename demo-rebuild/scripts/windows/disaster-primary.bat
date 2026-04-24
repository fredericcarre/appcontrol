@echo off
REM Simulate complete primary-site outage: remove all primary markers
set "PRIMARY_DIR=%TEMP%\appcontrol-demo-rebuild\primary"

if exist "%PRIMARY_DIR%" (
    del /Q "%PRIMARY_DIR%\*.running" 2>nul
    echo [DISASTER] Primary site: major outage simulated
    echo [DISASTER] All PRIMARY markers removed
    echo [DISASTER] Components will transition to FAILED within 10-30s
    echo.
    echo Next step:
    echo   1. Open the application in AppControl
    echo   2. Click "Switchover" -^> select DR site
    echo   3. Walk through the 6 phases
) else (
    echo [DISASTER] Directory %PRIMARY_DIR% not found. Run setup.bat first.
)
