@echo off
REM marker-check.bat <site> <name>
REM
REM Reports whether %TEMP%\appcontrol-demo-rebuild\<site>\<name>.running exists.
REM Used by the AppControl demo as check_cmd / integrity_check_cmd.
REM   exit 0 → marker present (component is RUNNING)
REM   exit 1 → marker missing (component is STOPPED/FAILED)
setlocal
if "%~1"=="" echo [marker-check] usage: marker-check.bat ^<site^> ^<name^>& exit /b 2
if "%~2"=="" echo [marker-check] usage: marker-check.bat ^<site^> ^<name^>& exit /b 2

set "FILE=%TEMP%\appcontrol-demo-rebuild\%~1\%~2.running"
if exist "%FILE%" (endlocal & exit /b 0) else (endlocal & exit /b 1)
