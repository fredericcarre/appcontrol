#Requires -Version 5.1
<#
.SYNOPSIS
    Deploy AppControl standalone on Windows (no Docker, no admin required)

.DESCRIPTION
    Deploys AppControl with configurable database backend:
    - sqlite: Single-file database, zero dependencies (recommended for portable deployment)
    - embedded: Portable PostgreSQL downloaded automatically
    - external: Connect to your own PostgreSQL server

.PARAMETER InstallDir
    Installation directory (default: current directory)

.PARAMETER DbMode
    Database mode:
    - 'sqlite'   : Single-file SQLite database (recommended, zero dependencies)
    - 'embedded' : Portable PostgreSQL (heavier, but full features)
    - 'external' : Your own PostgreSQL server

.EXAMPLE
    .\deploy-standalone.ps1 -DbMode sqlite
    .\deploy-standalone.ps1 -InstallDir C:\AppControl -DbMode embedded
    .\deploy-standalone.ps1 -DbMode external
#>

param(
    [string]$InstallDir = ".\AppControl",
    [ValidateSet("sqlite", "embedded", "external")]
    [string]$DbMode = "sqlite"
)

$ErrorActionPreference = "Stop"

# Versions
$PG_VERSION = "16.2"
$PG_PORTABLE_URL = "https://get.enterprisedb.com/postgresql/postgresql-$PG_VERSION-1-windows-x64-binaries.zip"

# ---------------------------------------------------------------------------
# Helper Functions
# ---------------------------------------------------------------------------

function Write-Status {
    param([string]$Message, [string]$Type = "INFO")
    $color = switch ($Type) {
        "INFO"    { "Cyan" }
        "SUCCESS" { "Green" }
        "WARNING" { "Yellow" }
        "ERROR"   { "Red" }
        default   { "White" }
    }
    $timestamp = Get-Date -Format "HH:mm:ss"
    Write-Host "[$timestamp][$Type] $Message" -ForegroundColor $color
}

function Ensure-Directory {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
    }
}

function Download-File {
    param([string]$Url, [string]$Output)

    if (Test-Path $Output) {
        Write-Status "Already exists: $Output"
        return
    }

    Write-Status "Downloading: $Url"

    # Use BITS for better reliability
    $job = Start-BitsTransfer -Source $Url -Destination $Output -Asynchronous

    while ($job.JobState -eq "Transferring" -or $job.JobState -eq "Connecting") {
        $percent = [int](($job.BytesTransferred / $job.BytesTotal) * 100)
        Write-Progress -Activity "Downloading" -Status "$percent%" -PercentComplete $percent
        Start-Sleep -Milliseconds 500
    }

    Complete-BitsTransfer -BitsJob $job
    Write-Progress -Activity "Downloading" -Completed
}

# ---------------------------------------------------------------------------
# Directory Structure
# ---------------------------------------------------------------------------

function Setup-Directories {
    Write-Status "Setting up directories..."

    Ensure-Directory $InstallDir
    Ensure-Directory "$InstallDir\bin"
    Ensure-Directory "$InstallDir\data"
    if ($DbMode -eq "embedded") {
        Ensure-Directory "$InstallDir\data\postgres"
    }
    Ensure-Directory "$InstallDir\logs"
    Ensure-Directory "$InstallDir\config"
    Ensure-Directory "$InstallDir\certs"
    Ensure-Directory "$InstallDir\frontend"

    Write-Status "Directories created" "SUCCESS"
}

# ---------------------------------------------------------------------------
# PostgreSQL Portable
# ---------------------------------------------------------------------------

function Install-PostgresPortable {
    $pgDir = "$InstallDir\pgsql"
    $pgBin = "$pgDir\bin"
    $pgData = "$InstallDir\data\postgres"

    if (Test-Path "$pgBin\pg_ctl.exe") {
        Write-Status "PostgreSQL already installed"
        return
    }

    Write-Status "Installing PostgreSQL Portable..."

    $zipFile = "$InstallDir\postgresql.zip"

    # Download
    Download-File -Url $PG_PORTABLE_URL -Output $zipFile

    # Extract
    Write-Status "Extracting PostgreSQL..."
    Expand-Archive -Path $zipFile -DestinationPath $InstallDir -Force

    # Cleanup
    Remove-Item $zipFile -Force

    # Initialize database
    Write-Status "Initializing database..."
    $env:PATH = "$pgBin;$env:PATH"

    & "$pgBin\initdb.exe" -D $pgData -U postgres -E UTF8 --locale=C 2>&1 | Out-Null

    if ($LASTEXITCODE -ne 0) {
        Write-Status "Failed to initialize database" "ERROR"
        exit 1
    }

    # Configure pg_hba.conf for local access only
    $hbaConf = @"
# TYPE  DATABASE        USER            ADDRESS                 METHOD
local   all             all                                     trust
host    all             all             127.0.0.1/32            md5
host    all             all             ::1/128                 md5
"@
    $hbaConf | Set-Content "$pgData\pg_hba.conf" -Encoding UTF8

    # Configure postgresql.conf
    $pgConf = Get-Content "$pgData\postgresql.conf"
    $pgConf = $pgConf -replace "#port = 5432", "port = 5433"  # Use non-standard port
    $pgConf = $pgConf -replace "#listen_addresses = 'localhost'", "listen_addresses = '127.0.0.1'"
    $pgConf | Set-Content "$pgData\postgresql.conf" -Encoding UTF8

    Write-Status "PostgreSQL installed" "SUCCESS"
}

function Start-PostgresPortable {
    $pgBin = "$InstallDir\pgsql\bin"
    $pgData = "$InstallDir\data\postgres"
    $logFile = "$InstallDir\logs\postgres.log"

    Write-Status "Starting PostgreSQL..."

    # Check if already running
    $pgProcess = Get-Process -Name "postgres" -ErrorAction SilentlyContinue
    if ($pgProcess) {
        Write-Status "PostgreSQL already running"
        return
    }

    & "$pgBin\pg_ctl.exe" start -D $pgData -l $logFile -w

    if ($LASTEXITCODE -ne 0) {
        Write-Status "Failed to start PostgreSQL" "ERROR"
        Get-Content $logFile -Tail 20
        exit 1
    }

    # Create appcontrol user and database
    Start-Sleep -Seconds 2

    $env:PGPASSWORD = "postgres"
    & "$pgBin\psql.exe" -h 127.0.0.1 -p 5433 -U postgres -c "CREATE USER appcontrol WITH PASSWORD 'appcontrol123' CREATEDB;" 2>$null
    & "$pgBin\psql.exe" -h 127.0.0.1 -p 5433 -U postgres -c "CREATE DATABASE appcontrol OWNER appcontrol;" 2>$null

    Write-Status "PostgreSQL started on port 5433" "SUCCESS"
}

function Stop-PostgresPortable {
    $pgBin = "$InstallDir\pgsql\bin"
    $pgData = "$InstallDir\data\postgres"

    Write-Status "Stopping PostgreSQL..."
    & "$pgBin\pg_ctl.exe" stop -D $pgData -m fast 2>$null
    Write-Status "PostgreSQL stopped" "SUCCESS"
}

# ---------------------------------------------------------------------------
# AppControl Binaries
# ---------------------------------------------------------------------------

function Download-AppControlBinaries {
    Write-Status "Downloading AppControl binaries..."

    $releases = "https://github.com/fredericcarre/appcontrol/releases/latest/download"
    $binDir = "$InstallDir\bin"

    # Backend (dual-mode: supports both PostgreSQL and SQLite via DATABASE_TYPE env var)
    if (-not (Test-Path "$binDir\appcontrol-backend.exe")) {
        try {
            Download-File -Url "$releases/appcontrol-backend-windows-amd64.exe" -Output "$binDir\appcontrol-backend.exe"
        } catch {
            Write-Status "Backend not available for download yet - will need manual copy" "WARNING"
        }
    }

    # Gateway
    if (-not (Test-Path "$binDir\appcontrol-gateway.exe")) {
        try {
            Download-File -Url "$releases/appcontrol-gateway-windows-amd64.exe" -Output "$binDir\appcontrol-gateway.exe"
        } catch {
            Write-Status "Gateway not available for download yet - will need manual copy" "WARNING"
        }
    }

    # Agent
    if (-not (Test-Path "$binDir\appcontrol-agent.exe")) {
        try {
            Download-File -Url "$releases/appcontrol-agent-windows-amd64.exe" -Output "$binDir\appcontrol-agent.exe"
        } catch {
            Write-Status "Agent not available for download yet - will need manual copy" "WARNING"
        }
    }

    Write-Status "Binaries ready" "SUCCESS"
}

function Download-Frontend {
    Write-Status "Downloading Frontend..."

    $releases = "https://github.com/fredericcarre/appcontrol/releases/latest/download"
    $frontendDir = "$InstallDir\frontend"
    $zipFile = "$InstallDir\frontend.zip"

    if (Test-Path "$frontendDir\index.html") {
        Write-Status "Frontend already downloaded"
        return
    }

    try {
        Download-File -Url "$releases/appcontrol-frontend.zip" -Output $zipFile
        Expand-Archive -Path $zipFile -DestinationPath $frontendDir -Force
        Remove-Item $zipFile -Force
    } catch {
        Write-Status "Frontend not available for download yet" "WARNING"
    }
}

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

function Create-Configuration {
    $configFile = "$InstallDir\config\appcontrol.yaml"

    if (Test-Path $configFile) {
        Write-Status "Configuration already exists"
        return
    }

    Write-Status "Creating configuration..."

    # Generate JWT secret
    $jwtSecret = [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes([guid]::NewGuid().ToString()))

    # Database configuration depends on mode
    if ($DbMode -eq "sqlite") {
        $dbConfig = @"
# Database - SQLite (portable, single-file)
database:
  type: sqlite
  path: ../data/appcontrol.db
"@
    } else {
        $dbConfig = @"
# Database - PostgreSQL
database:
  type: postgres
  url: postgresql://appcontrol:appcontrol123@127.0.0.1:5433/appcontrol
"@
    }

    $config = @"
# AppControl Configuration
# Generated on $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")
# Database mode: $DbMode

$dbConfig

# Backend
backend:
  listen: 127.0.0.1:3000
  jwt_secret: $jwtSecret
  log_level: info

# Frontend (served by backend)
frontend:
  static_dir: ../frontend

# Gateways - add your gateways here
gateways:
  - name: default
    zone: production
    listen: 0.0.0.0:8443
    backend_url: ws://127.0.0.1:3000/ws/gateway

# To add more gateways, copy the block above:
#  - name: disaster-recovery
#    zone: dr
#    listen: 0.0.0.0:8444
#    backend_url: ws://127.0.0.1:3000/ws/gateway
"@

    $config | Set-Content $configFile -Encoding UTF8

    Write-Status "Configuration created: $configFile" "SUCCESS"
    Write-Status "Edit this file to add more gateways" "INFO"
}

# ---------------------------------------------------------------------------
# Service Scripts
# ---------------------------------------------------------------------------

function Create-ServiceScripts {
    Write-Status "Creating service scripts..."

    if ($DbMode -eq "sqlite") {
        # SQLite mode: simpler scripts, no PostgreSQL
        $startScript = @'
@echo off
setlocal
cd /d "%~dp0"

echo Starting AppControl (SQLite mode)...
echo.

REM Set SQLite environment
set DATABASE_TYPE=sqlite
set SQLITE_PATH=%~dp0data\appcontrol.db

REM Start Backend
echo [1/2] Starting Backend...
start "AppControl Backend" /min cmd /c "bin\appcontrol-backend.exe > logs\backend.log 2>&1"
timeout /t 3 /nobreak >nul
echo Backend started on port 3000

REM Start Gateway
echo [2/2] Starting Gateway...
start "AppControl Gateway" /min cmd /c "bin\appcontrol-gateway.exe > logs\gateway.log 2>&1"
echo Gateway started on port 8443

echo.
echo ========================================
echo   AppControl is running!
echo ========================================
echo.
echo   Web UI: http://localhost:3000
echo   Gateway: localhost:8443
echo   Database: %~dp0data\appcontrol.db
echo.
echo   Logs: %~dp0logs\
echo.
echo Press any key to open the Web UI...
pause >nul
start http://localhost:3000
'@
        $stopScript = @'
@echo off
setlocal
cd /d "%~dp0"

echo Stopping AppControl...

REM Stop Gateway
echo Stopping Gateway...
taskkill /f /im appcontrol-gateway.exe 2>nul

REM Stop Backend
echo Stopping Backend...
taskkill /f /im appcontrol-backend.exe 2>nul

echo.
echo AppControl stopped.
pause
'@
        $statusScript = @'
@echo off
setlocal

echo AppControl Status (SQLite mode)
echo ================================
echo.

tasklist /fi "imagename eq appcontrol-backend.exe" 2>nul | find "appcontrol-backend" >nul
if errorlevel 1 (
    echo Backend:    STOPPED
) else (
    echo Backend:    RUNNING
)

tasklist /fi "imagename eq appcontrol-gateway.exe" 2>nul | find "appcontrol-gateway" >nul
if errorlevel 1 (
    echo Gateway:    STOPPED
) else (
    echo Gateway:    RUNNING
)

if exist "%~dp0data\appcontrol.db" (
    echo Database:   %~dp0data\appcontrol.db
) else (
    echo Database:   Not created yet (will be created on first start)
)

echo.
pause
'@
        $logsScript = @'
@echo off
cd /d "%~dp0"
start "Backend Logs" cmd /c "type logs\backend.log & pause"
start "Gateway Logs" cmd /c "type logs\gateway.log & pause"
'@
    } else {
        # PostgreSQL mode: include PostgreSQL management
        $startScript = @'
@echo off
setlocal
cd /d "%~dp0"

echo Starting AppControl...
echo.

REM Start PostgreSQL
echo [1/3] Starting PostgreSQL...
pgsql\bin\pg_ctl start -D data\postgres -l logs\postgres.log -w
if errorlevel 1 (
    echo ERROR: Failed to start PostgreSQL
    pause
    exit /b 1
)
echo PostgreSQL started on port 5433

REM Wait for DB
timeout /t 2 /nobreak >nul

REM Start Backend
echo [2/3] Starting Backend...
start "AppControl Backend" /min cmd /c "bin\appcontrol-backend.exe --config config\appcontrol.yaml > logs\backend.log 2>&1"
timeout /t 3 /nobreak >nul
echo Backend started on port 3000

REM Start Gateway
echo [3/3] Starting Gateway...
start "AppControl Gateway" /min cmd /c "bin\appcontrol-gateway.exe --config config\appcontrol.yaml > logs\gateway.log 2>&1"
echo Gateway started on port 8443

echo.
echo ========================================
echo   AppControl is running!
echo ========================================
echo.
echo   Web UI: http://localhost:3000
echo   Gateway: localhost:8443
echo.
echo   Logs: %~dp0logs\
echo.
echo Press any key to open the Web UI...
pause >nul
start http://localhost:3000
'@
        $stopScript = @'
@echo off
setlocal
cd /d "%~dp0"

echo Stopping AppControl...

REM Stop Gateway
echo Stopping Gateway...
taskkill /f /im appcontrol-gateway.exe 2>nul

REM Stop Backend
echo Stopping Backend...
taskkill /f /im appcontrol-backend.exe 2>nul

REM Stop PostgreSQL
echo Stopping PostgreSQL...
pgsql\bin\pg_ctl stop -D data\postgres -m fast 2>nul

echo.
echo AppControl stopped.
pause
'@
        $statusScript = @'
@echo off
setlocal

echo AppControl Status
echo =================
echo.

tasklist /fi "imagename eq postgres.exe" 2>nul | find "postgres" >nul
if errorlevel 1 (
    echo PostgreSQL: STOPPED
) else (
    echo PostgreSQL: RUNNING
)

tasklist /fi "imagename eq appcontrol-backend.exe" 2>nul | find "appcontrol-backend" >nul
if errorlevel 1 (
    echo Backend:    STOPPED
) else (
    echo Backend:    RUNNING
)

tasklist /fi "imagename eq appcontrol-gateway.exe" 2>nul | find "appcontrol-gateway" >nul
if errorlevel 1 (
    echo Gateway:    STOPPED
) else (
    echo Gateway:    RUNNING
)

echo.
pause
'@
        $logsScript = @'
@echo off
cd /d "%~dp0"
start "PostgreSQL Logs" cmd /c "type logs\postgres.log & pause"
start "Backend Logs" cmd /c "type logs\backend.log & pause"
start "Gateway Logs" cmd /c "type logs\gateway.log & pause"
'@
    }

    $startScript | Set-Content "$InstallDir\start.bat" -Encoding ASCII
    $stopScript | Set-Content "$InstallDir\stop.bat" -Encoding ASCII
    $statusScript | Set-Content "$InstallDir\status.bat" -Encoding ASCII
    $logsScript | Set-Content "$InstallDir\view-logs.bat" -Encoding ASCII

    Write-Status "Service scripts created" "SUCCESS"
}

# ---------------------------------------------------------------------------
# External Database Mode
# ---------------------------------------------------------------------------

function Configure-ExternalDb {
    Write-Status "External Database Configuration" "INFO"
    Write-Host ""

    $host = Read-Host "Database host (e.g., myserver.postgres.database.azure.com)"
    $port = Read-Host "Database port [5432]"
    if (-not $port) { $port = "5432" }
    $dbname = Read-Host "Database name [appcontrol]"
    if (-not $dbname) { $dbname = "appcontrol" }
    $user = Read-Host "Username"
    $pass = Read-Host "Password" -AsSecureString
    $passPlain = [Runtime.InteropServices.Marshal]::PtrToStringAuto(
        [Runtime.InteropServices.Marshal]::SecureStringToBSTR($pass)
    )

    $dbUrl = "postgresql://${user}:${passPlain}@${host}:${port}/${dbname}"

    # Update config
    $configFile = "$InstallDir\config\appcontrol.yaml"
    $config = Get-Content $configFile -Raw
    $config = $config -replace "url: postgresql://.*", "url: $dbUrl"
    $config | Set-Content $configFile -Encoding UTF8

    Write-Status "Database configuration updated" "SUCCESS"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  AppControl Standalone Deployment     " -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Installation directory: $InstallDir"
Write-Host "Database mode: $DbMode"
Write-Host ""

# Convert to absolute path
$InstallDir = [System.IO.Path]::GetFullPath($InstallDir)

# Setup
Setup-Directories

if ($DbMode -eq "embedded") {
    Install-PostgresPortable
}

Download-AppControlBinaries
Download-Frontend
Create-Configuration
Create-ServiceScripts

if ($DbMode -eq "external") {
    Configure-ExternalDb
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "  Installation Complete!               " -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
Write-Host ""
Write-Host "Installation directory: $InstallDir" -ForegroundColor Cyan
Write-Host ""
Write-Host "To start AppControl:" -ForegroundColor Yellow
Write-Host "  1. cd $InstallDir"
Write-Host "  2. .\start.bat"
Write-Host ""
Write-Host "To configure gateways:" -ForegroundColor Yellow
Write-Host "  Edit: $InstallDir\config\appcontrol.yaml"
Write-Host ""
Write-Host "Scripts available:" -ForegroundColor Yellow
Write-Host "  start.bat     - Start all services"
Write-Host "  stop.bat      - Stop all services"
Write-Host "  status.bat    - Check service status"
Write-Host "  view-logs.bat - View log files"
Write-Host ""

if ($DbMode -eq "sqlite") {
    Write-Host "NOTE: Using SQLite - single-file database at data\appcontrol.db" -ForegroundColor Yellow
    Write-Host "      No PostgreSQL required. Portable and lightweight." -ForegroundColor Yellow
} elseif ($DbMode -eq "embedded") {
    Write-Host "NOTE: PostgreSQL uses port 5433 (non-standard) to avoid conflicts" -ForegroundColor Yellow
}
