@echo off
REM Simulate corruption of JBoss member 003 (removes its marker)
set "DEMO_DIR=%TEMP%\appcontrol-demo-rebuild"
set "MARKER=%DEMO_DIR%\primary\jboss-003.running"

if exist "%MARKER%" (
    del /Q "%MARKER%"
    echo [CORRUPT] JBoss member 003: service down
    echo [CORRUPT] Marker removed: %MARKER%
    echo [CORRUPT] check_cmd will detect the failure within 10s
) else (
    echo [CORRUPT] JBoss 003 marker already absent - component already KO.
)
