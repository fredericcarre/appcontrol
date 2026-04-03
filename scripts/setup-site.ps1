#Requires -Version 5.1
<#
.SYNOPSIS
    Setup a complete AppControl site with gateway + agent on this machine.

.DESCRIPTION
    Interactive script that:
    1. Logs in to the backend (must be running)
    2. Creates a site (or reuses existing)
    3. Enrolls a gateway for that site
    4. Enrolls an agent connected to that gateway
    5. Saves enrollment state so subsequent runs just restart gateway + agent

.PARAMETER BackendUrl
    Backend URL (default: http://localhost:3000)

.PARAMETER Email
    Admin email (default: admin@localhost)

.PARAMETER Password
    Admin password (default: admin)

.PARAMETER SiteName
    Site name (will prompt if not provided)

.EXAMPLE
    .\setup-site.ps1
    .\setup-site.ps1 -SiteName "Production" -Email "admin@localhost" -Password "admin"
#>

param(
    [string]$BackendUrl = "http://localhost:3000",
    [string]$Email = "admin@localhost",
    [string]$Password = "admin",
    [string]$SiteName
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$StateFile = Join-Path $ScriptDir "site-state.json"
$BinDir = Join-Path $ScriptDir "bin"
$LogsDir = Join-Path $ScriptDir "logs"

# ---------------------------------------------------------------------------
# Helpers
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

function Invoke-Api {
    param(
        [string]$Method = "GET",
        [string]$Path,
        [object]$Body,
        [string]$Token
    )
    $uri = "$BackendUrl/api/v1$Path"
    $headers = @{}
    if ($Token) { $headers["Authorization"] = "Bearer $Token" }

    $params = @{
        Uri         = $uri
        Method      = $Method
        ContentType = "application/json"
        Headers     = $headers
    }
    if ($Body) {
        $params["Body"] = ($Body | ConvertTo-Json -Depth 10)
    }

    try {
        $response = Invoke-RestMethod @params
        return $response
    } catch {
        $status = $_.Exception.Response.StatusCode.value__
        $detail = $_.ErrorDetails.Message
        if ($status -eq 409) {
            # Conflict = already exists, not an error
            return $null
        }
        Write-Status "API error: $Method $Path → $status $detail" "ERROR"
        throw
    }
}

function Invoke-Enroll {
    param(
        [string]$Token,
        [string]$Hostname
    )
    $uri = "$BackendUrl/api/v1/enroll"
    $body = @{
        token    = $Token
        hostname = $Hostname
    } | ConvertTo-Json

    try {
        $response = Invoke-RestMethod -Uri $uri -Method POST -ContentType "application/json" -Body $body
        return $response
    } catch {
        $status = $_.Exception.Response.StatusCode.value__
        Write-Status "Enrollment failed: $status" "ERROR"
        throw
    }
}

# ---------------------------------------------------------------------------
# Check prerequisites
# ---------------------------------------------------------------------------

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  AppControl Site Setup" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Check backend is running
try {
    $health = Invoke-RestMethod -Uri "$BackendUrl/health" -TimeoutSec 5
    Write-Status "Backend is running at $BackendUrl" "SUCCESS"
} catch {
    Write-Status "Backend is not running at $BackendUrl" "ERROR"
    Write-Host "Start the backend first: .\start.bat" -ForegroundColor Yellow
    exit 1
}

# Check binaries exist
$gwBin = Join-Path $BinDir "appcontrol-gateway.exe"
$agentBin = Join-Path $BinDir "appcontrol-agent.exe"

if (-not (Test-Path $gwBin)) {
    Write-Status "Gateway binary not found: $gwBin" "ERROR"
    exit 1
}
if (-not (Test-Path $agentBin)) {
    Write-Status "Agent binary not found: $agentBin" "ERROR"
    exit 1
}

New-Item -ItemType Directory -Force -Path $LogsDir | Out-Null

# ---------------------------------------------------------------------------
# Check if already setup (restart mode)
# ---------------------------------------------------------------------------

if (Test-Path $StateFile) {
    $state = Get-Content $StateFile | ConvertFrom-Json
    Write-Status "Found existing setup for site '$($state.site_name)'" "INFO"
    Write-Host ""

    $restart = Read-Host "Restart gateway + agent? (Y/n)"
    if ($restart -ne "n" -and $restart -ne "N") {
        Write-Status "Starting gateway..." "INFO"

        $gwEnv = @{
            BACKEND_URL              = "ws://$($BackendUrl -replace 'http://','')/ws/gateway"
            GATEWAY_ENROLLMENT_TOKEN = $state.gateway_token
            GATEWAY_ZONE             = $state.site_code
            RUST_LOG                 = "info"
        }
        $gwProcess = Start-Process -FilePath $gwBin -PassThru -WindowStyle Normal -RedirectStandardError (Join-Path $LogsDir "gateway.log")
        foreach ($k in $gwEnv.Keys) {
            [System.Environment]::SetEnvironmentVariable($k, $gwEnv[$k], "Process")
        }

        Start-Sleep -Seconds 3
        Write-Status "Gateway started (PID: $($gwProcess.Id))" "SUCCESS"

        Write-Status "Starting agent..." "INFO"

        $env:GATEWAY_URL = "ws://localhost:4443"
        $env:AGENT_ENROLLMENT_TOKEN = $state.agent_token
        $env:RUST_LOG = "info"

        $agentProcess = Start-Process -FilePath $agentBin -PassThru -WindowStyle Normal -RedirectStandardError (Join-Path $LogsDir "agent.log")
        Start-Sleep -Seconds 2
        Write-Status "Agent started (PID: $($agentProcess.Id))" "SUCCESS"

        Write-Host ""
        Write-Host "========================================" -ForegroundColor Green
        Write-Host "  Site '$($state.site_name)' is running!" -ForegroundColor Green
        Write-Host "========================================" -ForegroundColor Green
        Write-Host ""
        Write-Host "  Gateway PID: $($gwProcess.Id)"
        Write-Host "  Agent PID:   $($agentProcess.Id)"
        Write-Host "  Web UI:      $BackendUrl"
        Write-Host ""
        exit 0
    }
}

# ---------------------------------------------------------------------------
# Login
# ---------------------------------------------------------------------------

Write-Status "Logging in as $Email..." "INFO"
$loginResp = Invoke-Api -Method POST -Path "/auth/login" -Body @{
    email    = $Email
    password = $Password
}
$token = $loginResp.token
Write-Status "Logged in successfully" "SUCCESS"

# ---------------------------------------------------------------------------
# Create or select site
# ---------------------------------------------------------------------------

if (-not $SiteName) {
    Write-Host ""
    $SiteName = Read-Host "Enter site name (e.g., Production, DR-Site)"
}

$siteCode = ($SiteName -replace '[^a-zA-Z0-9]', '-').ToUpper().Substring(0, [Math]::Min($SiteName.Length, 10))

Write-Status "Creating site '$SiteName' (code: $siteCode)..." "INFO"

# Check if site already exists
$sites = Invoke-Api -Method GET -Path "/sites" -Token $token
$existingSite = $null

if ($sites -is [array]) {
    $existingSite = $sites | Where-Object { $_.name -eq $SiteName } | Select-Object -First 1
} elseif ($sites.sites) {
    $existingSite = $sites.sites | Where-Object { $_.name -eq $SiteName } | Select-Object -First 1
}

if ($existingSite) {
    $siteId = $existingSite.id
    Write-Status "Site '$SiteName' already exists (ID: $siteId)" "INFO"
} else {
    $site = Invoke-Api -Method POST -Path "/sites" -Token $token -Body @{
        name      = $SiteName
        code      = $siteCode
        site_type = "primary"
    }
    $siteId = $site.id
    Write-Status "Site '$SiteName' created (ID: $siteId)" "SUCCESS"
}

# ---------------------------------------------------------------------------
# Create gateway enrollment token
# ---------------------------------------------------------------------------

Write-Status "Creating gateway enrollment token..." "INFO"
$gwTokenResp = Invoke-Api -Method POST -Path "/enrollment/tokens" -Token $token -Body @{
    name       = "Gateway-$SiteName-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
    scope      = "gateway"
    max_uses   = 1
    valid_hours = 8760
}
$gwEnrollToken = $gwTokenResp.token
Write-Status "Gateway enrollment token created" "SUCCESS"

# ---------------------------------------------------------------------------
# Create agent enrollment token
# ---------------------------------------------------------------------------

Write-Status "Creating agent enrollment token..." "INFO"
$agentTokenResp = Invoke-Api -Method POST -Path "/enrollment/tokens" -Token $token -Body @{
    name       = "Agent-$SiteName-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
    scope      = "agent"
    max_uses   = 10
    valid_hours = 8760
}
$agentEnrollToken = $agentTokenResp.token
Write-Status "Agent enrollment token created" "SUCCESS"

# ---------------------------------------------------------------------------
# Save state for future restarts
# ---------------------------------------------------------------------------

$state = @{
    site_name     = $SiteName
    site_id       = $siteId
    site_code     = $siteCode
    gateway_token = $gwEnrollToken
    agent_token   = $agentEnrollToken
    created_at    = (Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ")
}
$state | ConvertTo-Json | Set-Content $StateFile
Write-Status "State saved to $StateFile" "INFO"

# ---------------------------------------------------------------------------
# Start gateway
# ---------------------------------------------------------------------------

Write-Status "Starting gateway with enrollment..." "INFO"

$env:BACKEND_URL = "ws://$($BackendUrl -replace 'http://','')/ws/gateway"
$env:GATEWAY_ENROLLMENT_TOKEN = $gwEnrollToken
$env:GATEWAY_ZONE = $siteCode
$env:RUST_LOG = "info"

$gwProcess = Start-Process -FilePath $gwBin -PassThru -WindowStyle Normal
Start-Sleep -Seconds 5

Write-Status "Gateway started (PID: $($gwProcess.Id))" "SUCCESS"

# ---------------------------------------------------------------------------
# Assign gateway to site (after enrollment)
# ---------------------------------------------------------------------------

Write-Status "Assigning gateway to site '$SiteName'..." "INFO"
Start-Sleep -Seconds 2

$gateways = Invoke-Api -Method GET -Path "/gateways" -Token $token
$gwList = if ($gateways.gateways) { $gateways.gateways } elseif ($gateways -is [array]) { $gateways } else { @() }

$myGw = $gwList | Sort-Object -Property created_at -Descending | Select-Object -First 1
if ($myGw) {
    try {
        Invoke-Api -Method PUT -Path "/gateways/$($myGw.id)" -Token $token -Body @{
            site_id = $siteId
        }
        Write-Status "Gateway assigned to site '$SiteName'" "SUCCESS"
    } catch {
        Write-Status "Could not assign gateway to site (may already be assigned)" "WARNING"
    }
} else {
    Write-Status "Gateway not yet visible — it will appear after enrollment completes" "WARNING"
}

# ---------------------------------------------------------------------------
# Start agent
# ---------------------------------------------------------------------------

Write-Status "Starting agent with enrollment..." "INFO"

$env:GATEWAY_URL = "ws://localhost:4443"
$env:AGENT_ENROLLMENT_TOKEN = $agentEnrollToken
$env:RUST_LOG = "info"

$agentProcess = Start-Process -FilePath $agentBin -PassThru -WindowStyle Normal
Start-Sleep -Seconds 3

Write-Status "Agent started (PID: $($agentProcess.Id))" "SUCCESS"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "  Setup Complete!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
Write-Host ""
Write-Host "  Site:      $SiteName ($siteCode)"
Write-Host "  Gateway:   PID $($gwProcess.Id) (enrolled)"
Write-Host "  Agent:     PID $($agentProcess.Id) (enrolled)"
Write-Host "  Web UI:    $BackendUrl"
Write-Host ""
Write-Host "  To restart later: .\setup-site.ps1" -ForegroundColor Yellow
Write-Host "  (will detect existing enrollment and just restart services)" -ForegroundColor Yellow
Write-Host ""
Write-Host "  To add another site: .\setup-site.ps1 -SiteName 'DR-Site'" -ForegroundColor Yellow
Write-Host ""
