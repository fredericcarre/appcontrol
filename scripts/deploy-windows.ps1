#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Deploy AppControl on Windows

.DESCRIPTION
    This script deploys AppControl components on a Windows machine.
    It supports Docker Desktop or native PostgreSQL installation.

.PARAMETER ConfigFile
    Path to the configuration file (default: appcontrol-config.json)

.PARAMETER Mode
    Deployment mode: 'docker' (default) or 'native'

.EXAMPLE
    .\deploy-windows.ps1 -Mode docker
    .\deploy-windows.ps1 -ConfigFile .\my-config.json -Mode native
#>

param(
    [string]$ConfigFile = ".\appcontrol-config.json",
    [ValidateSet("docker", "native")]
    [string]$Mode = "docker"
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

# Default configuration
$DefaultConfig = @{
    # Database
    database = @{
        host = "localhost"
        port = 5432
        name = "appcontrol"
        user = "appcontrol"
        password = "changeme"  # CHANGE THIS!
    }

    # Backend
    backend = @{
        port = 3000
        jwt_secret = [guid]::NewGuid().ToString()
    }

    # Frontend
    frontend = @{
        port = 8080
    }

    # Gateways - add your gateways here
    gateways = @(
        @{
            name = "default"
            zone = "production"
            port = 8443
            backend_url = "ws://localhost:3000/ws/gateway"
        }
    )

    # Docker images (for docker mode)
    images = @{
        backend = "ghcr.io/fredericcarre/appcontrol-backend:latest"
        frontend = "ghcr.io/fredericcarre/appcontrol-frontend:latest"
        gateway = "ghcr.io/fredericcarre/appcontrol-gateway:latest"
        postgres = "postgres:16-alpine"
    }
}

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
    Write-Host "[$Type] $Message" -ForegroundColor $color
}

function Test-Command {
    param([string]$Command)
    $null = Get-Command $Command -ErrorAction SilentlyContinue
    return $?
}

function Get-OrCreateConfig {
    if (Test-Path $ConfigFile) {
        Write-Status "Loading configuration from $ConfigFile"
        $config = Get-Content $ConfigFile | ConvertFrom-Json -AsHashtable
        return $config
    }
    else {
        Write-Status "Creating default configuration file: $ConfigFile" "WARNING"
        $DefaultConfig | ConvertTo-Json -Depth 10 | Set-Content $ConfigFile
        Write-Status "Please edit $ConfigFile and run again" "WARNING"
        Write-Status "IMPORTANT: Change the database password!" "WARNING"
        return $null
    }
}

# ---------------------------------------------------------------------------
# Docker Mode
# ---------------------------------------------------------------------------

function Deploy-Docker {
    param($Config)

    Write-Status "Deploying with Docker..."

    # Check Docker
    if (-not (Test-Command "docker")) {
        Write-Status "Docker not found. Please install Docker Desktop." "ERROR"
        Write-Status "Download: https://www.docker.com/products/docker-desktop/" "INFO"
        exit 1
    }

    # Check Docker is running
    $dockerInfo = docker info 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Status "Docker is not running. Please start Docker Desktop." "ERROR"
        exit 1
    }

    Write-Status "Docker is running" "SUCCESS"

    # Create docker-compose.yaml
    $composeFile = Join-Path $ScriptDir "docker-compose.windows.yaml"
    $composeContent = Generate-DockerCompose $Config
    $composeContent | Set-Content $composeFile -Encoding UTF8

    Write-Status "Generated docker-compose.windows.yaml"

    # Create data directory
    $dataDir = Join-Path $ScriptDir "data"
    if (-not (Test-Path $dataDir)) {
        New-Item -ItemType Directory -Path $dataDir | Out-Null
    }

    # Pull images
    Write-Status "Pulling Docker images..."
    docker compose -f $composeFile pull

    # Start services
    Write-Status "Starting services..."
    docker compose -f $composeFile up -d

    if ($LASTEXITCODE -eq 0) {
        Write-Status "AppControl deployed successfully!" "SUCCESS"
        Write-Status ""
        Write-Status "Access the UI at: http://localhost:$($Config.frontend.port)" "INFO"
        Write-Status "Backend API at: http://localhost:$($Config.backend.port)" "INFO"
        foreach ($gw in $Config.gateways) {
            Write-Status "Gateway '$($gw.name)' listening on port $($gw.port)" "INFO"
        }
        Write-Status ""
        Write-Status "To view logs: docker compose -f $composeFile logs -f" "INFO"
        Write-Status "To stop: docker compose -f $composeFile down" "INFO"
    }
    else {
        Write-Status "Deployment failed" "ERROR"
        exit 1
    }
}

function Generate-DockerCompose {
    param($Config)

    $db = $Config.database
    $be = $Config.backend
    $fe = $Config.frontend

    $yaml = @"
version: '3.8'

services:
  postgres:
    image: $($Config.images.postgres)
    container_name: appcontrol-postgres
    environment:
      POSTGRES_DB: $($db.name)
      POSTGRES_USER: $($db.user)
      POSTGRES_PASSWORD: $($db.password)
    volumes:
      - ./data/postgres:/var/lib/postgresql/data
    ports:
      - "$($db.port):5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U $($db.user) -d $($db.name)"]
      interval: 5s
      timeout: 5s
      retries: 5

  backend:
    image: $($Config.images.backend)
    container_name: appcontrol-backend
    depends_on:
      postgres:
        condition: service_healthy
    environment:
      DATABASE_URL: postgresql://$($db.user):$($db.password)@postgres:5432/$($db.name)
      JWT_SECRET: $($be.jwt_secret)
      RUST_LOG: info
    ports:
      - "$($be.port):3000"

  frontend:
    image: $($Config.images.frontend)
    container_name: appcontrol-frontend
    depends_on:
      - backend
    environment:
      VITE_API_URL: http://localhost:$($be.port)
    ports:
      - "$($fe.port):80"

"@

    # Add gateways
    foreach ($gw in $Config.gateways) {
        $yaml += @"

  gateway-$($gw.name):
    image: $($Config.images.gateway)
    container_name: appcontrol-gateway-$($gw.name)
    depends_on:
      - backend
    environment:
      GATEWAY_NAME: $($gw.name)
      GATEWAY_ZONE: $($gw.zone)
      BACKEND_URL: ws://backend:3000/ws/gateway
      LISTEN_PORT: 8443
      RUST_LOG: info
    ports:
      - "$($gw.port):8443"

"@
    }

    return $yaml
}

# ---------------------------------------------------------------------------
# Native Mode (without Docker)
# ---------------------------------------------------------------------------

function Deploy-Native {
    param($Config)

    Write-Status "Deploying in native mode..."
    Write-Status "This mode requires:" "WARNING"
    Write-Status "  - PostgreSQL 16 installed and running" "WARNING"
    Write-Status "  - Pre-built binaries for Windows" "WARNING"

    $db = $Config.database

    # Check PostgreSQL
    if (-not (Test-Command "psql")) {
        Write-Status "PostgreSQL not found in PATH" "ERROR"
        Write-Status "Please install PostgreSQL 16: https://www.postgresql.org/download/windows/" "INFO"
        exit 1
    }

    # Test connection
    Write-Status "Testing database connection..."
    $env:PGPASSWORD = $db.password
    $result = psql -h $db.host -p $db.port -U $db.user -d postgres -c "SELECT 1" 2>&1

    if ($LASTEXITCODE -ne 0) {
        Write-Status "Cannot connect to PostgreSQL" "ERROR"
        Write-Status "Please check your database configuration" "INFO"
        exit 1
    }

    # Create database if not exists
    Write-Status "Creating database if not exists..."
    psql -h $db.host -p $db.port -U $db.user -d postgres -c "CREATE DATABASE $($db.name)" 2>$null

    Write-Status "Database ready" "SUCCESS"

    # Download binaries
    $binDir = Join-Path $ScriptDir "bin"
    if (-not (Test-Path $binDir)) {
        New-Item -ItemType Directory -Path $binDir | Out-Null
    }

    Write-Status "Downloading AppControl binaries..."
    $releases = "https://github.com/fredericcarre/appcontrol/releases/latest/download"

    # Download backend
    $backendExe = Join-Path $binDir "appcontrol-backend.exe"
    if (-not (Test-Path $backendExe)) {
        Write-Status "Downloading backend..."
        Invoke-WebRequest -Uri "$releases/appcontrol-backend-windows-amd64.exe" -OutFile $backendExe
    }

    # Download gateway
    $gatewayExe = Join-Path $binDir "appcontrol-gateway.exe"
    if (-not (Test-Path $gatewayExe)) {
        Write-Status "Downloading gateway..."
        Invoke-WebRequest -Uri "$releases/appcontrol-gateway-windows-amd64.exe" -OutFile $gatewayExe
    }

    # Create start script
    $startScript = Join-Path $ScriptDir "start-appcontrol.ps1"
    Generate-StartScript $Config $startScript

    Write-Status "Native deployment prepared" "SUCCESS"
    Write-Status ""
    Write-Status "To start AppControl, run: .\start-appcontrol.ps1" "INFO"
}

function Generate-StartScript {
    param($Config, $OutputPath)

    $db = $Config.database
    $be = $Config.backend

    $script = @"
# Start AppControl Services
`$ErrorActionPreference = "Stop"
`$ScriptDir = Split-Path -Parent `$MyInvocation.MyCommand.Path
`$BinDir = Join-Path `$ScriptDir "bin"

# Environment
`$env:DATABASE_URL = "postgresql://$($db.user):$($db.password)@$($db.host):$($db.port)/$($db.name)"
`$env:JWT_SECRET = "$($be.jwt_secret)"
`$env:RUST_LOG = "info"

# Start Backend
Write-Host "Starting Backend..."
Start-Process -FilePath (Join-Path `$BinDir "appcontrol-backend.exe") -NoNewWindow

# Wait for backend
Start-Sleep -Seconds 5

# Start Gateways
"@

    foreach ($gw in $Config.gateways) {
        $script += @"

Write-Host "Starting Gateway: $($gw.name)..."
`$env:GATEWAY_NAME = "$($gw.name)"
`$env:GATEWAY_ZONE = "$($gw.zone)"
`$env:BACKEND_URL = "$($gw.backend_url)"
`$env:LISTEN_PORT = "$($gw.port)"
Start-Process -FilePath (Join-Path `$BinDir "appcontrol-gateway.exe") -NoNewWindow

"@
    }

    $script += @"

Write-Host ""
Write-Host "AppControl is running!" -ForegroundColor Green
Write-Host "Backend: http://localhost:$($be.port)"
Write-Host ""
Write-Host "Press Ctrl+C to stop..."
Wait-Process -Name "appcontrol-backend"
"@

    $script | Set-Content $OutputPath -Encoding UTF8
}

# ---------------------------------------------------------------------------
# Gateway Configuration Helper
# ---------------------------------------------------------------------------

function Add-Gateway {
    param($Config)

    Write-Host ""
    Write-Host "=== Add Gateway ===" -ForegroundColor Cyan

    $name = Read-Host "Gateway name (e.g., 'datacenter-paris')"
    $zone = Read-Host "Zone (e.g., 'production', 'disaster-recovery')"
    $port = Read-Host "Port (default: 8443)"
    if (-not $port) { $port = 8443 }

    $gateway = @{
        name = $name
        zone = $zone
        port = [int]$port
        backend_url = "ws://localhost:$($Config.backend.port)/ws/gateway"
    }

    $Config.gateways += $gateway

    Write-Status "Gateway '$name' added" "SUCCESS"
    return $Config
}

function Show-Menu {
    param($Config)

    while ($true) {
        Write-Host ""
        Write-Host "=== AppControl Configuration ===" -ForegroundColor Cyan
        Write-Host "1. Deploy with current configuration"
        Write-Host "2. Add a gateway"
        Write-Host "3. List gateways"
        Write-Host "4. Change database settings"
        Write-Host "5. Save configuration"
        Write-Host "6. Exit"
        Write-Host ""

        $choice = Read-Host "Choice"

        switch ($choice) {
            "1" {
                # Save config first
                $Config | ConvertTo-Json -Depth 10 | Set-Content $ConfigFile

                if ($Mode -eq "docker") {
                    Deploy-Docker $Config
                } else {
                    Deploy-Native $Config
                }
                return
            }
            "2" {
                $Config = Add-Gateway $Config
            }
            "3" {
                Write-Host ""
                Write-Host "Configured Gateways:" -ForegroundColor Cyan
                foreach ($gw in $Config.gateways) {
                    Write-Host "  - $($gw.name) (zone: $($gw.zone), port: $($gw.port))"
                }
            }
            "4" {
                $Config.database.host = Read-Host "Database host [$($Config.database.host)]"
                if (-not $Config.database.host) { $Config.database.host = "localhost" }

                $port = Read-Host "Database port [$($Config.database.port)]"
                if ($port) { $Config.database.port = [int]$port }

                $Config.database.password = Read-Host "Database password" -AsSecureString |
                    ConvertFrom-SecureString -AsPlainText

                Write-Status "Database settings updated" "SUCCESS"
            }
            "5" {
                $Config | ConvertTo-Json -Depth 10 | Set-Content $ConfigFile
                Write-Status "Configuration saved to $ConfigFile" "SUCCESS"
            }
            "6" {
                return
            }
        }
    }
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "   AppControl Windows Deployment Tool  " -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Load or create config
$Config = Get-OrCreateConfig
if ($null -eq $Config) {
    exit 0
}

# Interactive menu
Show-Menu $Config
