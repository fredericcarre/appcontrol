@echo off
REM Live view of primary/dr markers + last XL invocations - keep open in a side terminal
setlocal
set "DEMO_DIR=%TEMP%\appcontrol-demo-rebuild"

:loop
cls
echo ===================================================================
echo  AppControl Critical-App Demo - Live markers  [%TIME%]
echo ===================================================================
echo.
echo -- PRIMARY site : %DEMO_DIR%\primary\
if exist "%DEMO_DIR%\primary" (
    dir /b /a-d "%DEMO_DIR%\primary\*.running" 2^>nul || echo   ^(no service running^)
) else (
    echo   (directory missing - run setup.bat)
)
echo.
echo -- DR site      : %DEMO_DIR%\dr\
if exist "%DEMO_DIR%\dr" (
    dir /b /a-d "%DEMO_DIR%\dr\*.running" 2^>nul || echo   ^(no service running^)
) else (
    echo   (directory missing - run setup.bat)
)
echo.
echo -- XL DEPLOY LOG (last calls):
if exist "%DEMO_DIR%\xldeploy.log" (
    powershell -NoProfile -Command "Get-Content '%DEMO_DIR%\xldeploy.log' -Tail 3"
) else (
    echo   (no XL Deploy call yet)
)
echo.
echo -- XL RELEASE LOG (last calls):
if exist "%DEMO_DIR%\xlrelease.log" (
    powershell -NoProfile -Command "Get-Content '%DEMO_DIR%\xlrelease.log' -Tail 3"
) else (
    echo   (no XL Release call yet)
)
echo.
echo ===================================================================
echo Ctrl+C to quit. Refresh every 2s.
timeout /t 2 /nobreak >nul
goto loop
