@echo off
REM Reset: wipe all markers + logs, ready for a clean demo run
set "DEMO_DIR=%TEMP%\appcontrol-demo-rebuild"

if exist "%DEMO_DIR%\primary" del /Q "%DEMO_DIR%\primary\*.running" 2>nul
if exist "%DEMO_DIR%\dr"      del /Q "%DEMO_DIR%\dr\*.running"      2>nul
if exist "%DEMO_DIR%\xldeploy.log"  del /Q "%DEMO_DIR%\xldeploy.log"  2>nul
if exist "%DEMO_DIR%\xlrelease.log" del /Q "%DEMO_DIR%\xlrelease.log" 2>nul

echo [RESET] Markers and logs cleared. Clean state ready.
echo.
echo You can now:
echo   - From AppControl: click "Start app" on the Critical Banking App map
echo   - Or re-run the full demo scenario
