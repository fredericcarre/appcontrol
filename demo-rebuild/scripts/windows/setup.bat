@echo off
REM Setup - creates directories, installs mock CLIs, shows status
setlocal

set "DEMO_DIR=%TEMP%\appcontrol-demo-rebuild"
set "BIN_DIR=%DEMO_DIR%\bin"

echo [SETUP] Critical-app demo: creating directories and mock CLIs
echo.

if not exist "%DEMO_DIR%\primary" mkdir "%DEMO_DIR%\primary"
if not exist "%DEMO_DIR%\dr"      mkdir "%DEMO_DIR%\dr"
if not exist "%BIN_DIR%"          mkdir "%BIN_DIR%"

copy /Y "%~dp0xldeploy-cli.bat" "%BIN_DIR%\xldeploy-cli.bat" >nul
copy /Y "%~dp0xlr-cli.bat"      "%BIN_DIR%\xlr-cli.bat"      >nul

echo [SETUP] Directories created:
echo   %DEMO_DIR%\primary    (primary site markers)
echo   %DEMO_DIR%\dr         (DR site markers)
echo   %BIN_DIR%             (mock XL Deploy / XL Release)
echo.
echo [SETUP] Mock CLIs installed in %BIN_DIR%
echo.
echo Ready for demo. In a separate terminal:
echo   %~dp0watch-markers.bat        watch markers live
echo   %~dp0corrupt-jboss-003.bat    simulate one member down
echo   %~dp0disaster-primary.bat     simulate primary-site outage
echo   %~dp0reset.bat                reset to clean state
echo.
endlocal
