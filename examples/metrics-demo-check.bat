@echo off
REM AppControl Metrics Demo - Health Check Script
REM Usage: metrics-demo-check.bat <component-name>
REM Returns exit 0 + JSON metrics if running, exit 1 if stopped

set COMP=%1
set FLAG=%TEMP%\appcontrol_demo\%COMP%.running
set METRICS=%TEMP%\appcontrol_demo\%COMP%.json

if not exist "%FLAG%" exit /b 1
if exist "%METRICS%" (
    type "%METRICS%"
) else (
    echo {"status": 1}
)
exit /b 0
