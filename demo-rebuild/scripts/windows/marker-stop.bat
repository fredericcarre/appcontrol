@echo off
REM marker-stop.bat <site> <name>
REM
REM Idempotently removes %TEMP%\appcontrol-demo-rebuild\<site>\<name>.running.
REM Used by the AppControl demo as stop_cmd. Always exits 0 — stopping a
REM component that's already stopped is not an error in our FSM.
setlocal
if "%~1"=="" echo [marker-stop] usage: marker-stop.bat ^<site^> ^<name^>& exit /b 2
if "%~2"=="" echo [marker-stop] usage: marker-stop.bat ^<site^> ^<name^>& exit /b 2

set "FILE=%TEMP%\appcontrol-demo-rebuild\%~1\%~2.running"
if exist "%FILE%" del /f /q "%FILE%" >nul 2>&1
endlocal & exit /b 0
