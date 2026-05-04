@echo off
REM marker-start.bat <site> <name>
REM
REM Idempotently creates %TEMP%\appcontrol-demo-rebuild\<site>\<name>.running.
REM Used by the AppControl demo as start_cmd.
REM
REM Always exits 0 unless the directory cannot be created (permission issue).
REM Wrapping the marker logic in a .bat avoids the cmd.exe parser quirks
REM around chained `if ... mkdir & echo.>` (which the agent invokes via
REM `cmd /C` and which silently misbehaves on some Windows builds).
setlocal
if "%~1"=="" echo [marker-start] usage: marker-start.bat ^<site^> ^<name^>& exit /b 2
if "%~2"=="" echo [marker-start] usage: marker-start.bat ^<site^> ^<name^>& exit /b 2

set "DEMO_DIR=%TEMP%\appcontrol-demo-rebuild"
set "DIR=%DEMO_DIR%\%~1"
set "FILE=%DIR%\%~2.running"

if not exist "%DIR%" mkdir "%DIR%" >nul 2>&1
if not exist "%DIR%" (
    echo [marker-start] FAILED to create directory %DIR% 1>&2
    exit /b 1
)

type nul > "%FILE%"
if not exist "%FILE%" (
    echo [marker-start] FAILED to create marker %FILE% 1>&2
    exit /b 1
)

endlocal & exit /b 0
