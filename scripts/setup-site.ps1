#Requires -Version 5.1
<#
.SYNOPSIS
    Setup a complete AppControl site with gateway + agent on this machine.

.DESCRIPTION
    Stateless script — queries the backend for everything:
    1. Logs in to the backend (must be running)
    2. Creates a site if it doesn't exist
    3. Checks if a gateway is already registered for this site
    4. If not: creates enrollment token, launches gateway, assigns to site
    5. Checks if an agent is connected to this gateway
    6. If not: creates enrollment token, launches agent
    7. If already setup: just restarts gateway + agent

.PARAMETER BackendUrl
    Backend URL (default: http://localhost:3000)

.PARAMETER Email
    Admin email (default: admin@localhost)

.PARAMETER Password
    Admin password (default: admin)

.PARAMETER SiteName
    Site name (will prompt if not provided)

.PARAMETER GatewayPort
    Gateway listen port (default: 4443)

.EXAMPLE
    .\setup-site.ps1
    .\setup-site.ps1 -SiteName "Production"
    .\setup-site.ps1 -SiteName "DR-Site" -GatewayPort 4444
#>

param(
    [string]$BackendUrl = "http://localhost:3000",
    [string]$Email = "admin@localhost",
    [string]$Password = "admin",
    [string]$SiteName,
    [int]$GatewayPort = 4443
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
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
        return Invoke-RestMethod @params
    } catch {
        $status = $_.Exception.Response.StatusCode.value__
        $detail = $_.ErrorDetails.Message
        if ($status -eq 409) { return $null }  # Already exists
        if ($status -eq 404) { return $null }  # Not found
        Write-Status "API error: $Method $Path -> $status $detail" "ERROR"
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
    Invoke-RestMethod -Uri "$BackendUrl/health" -TimeoutSec 5 | Out-Null
    Write-Status "Backend running at $BackendUrl" "SUCCESS"
} catch {
    Write-Status "Backend not running at $BackendUrl — start it first" "ERROR"
    exit 1
}

# Check binaries
$gwBin = Join-Path $BinDir "appcontrol-gateway.exe"
$agentBin = Join-Path $BinDir "appcontrol-agent.exe"

if (-not (Test-Path $gwBin)) { Write-Status "Gateway binary not found: $gwBin" "ERROR"; exit 1 }
if (-not (Test-Path $agentBin)) { Write-Status "Agent binary not found: $agentBin" "ERROR"; exit 1 }

New-Item -ItemType Directory -Force -Path $LogsDir | Out-Null

# ---------------------------------------------------------------------------
# Login
# ---------------------------------------------------------------------------

Write-Status "Logging in as $Email..." "INFO"
$loginResp = Invoke-Api -Method POST -Path "/auth/login" -Body @{
    email    = $Email
    password = $Password
}
if (-not $loginResp.token) {
    Write-Status "Login failed — check email/password" "ERROR"
    exit 1
}
$token = $loginResp.token
Write-Status "Logged in" "SUCCESS"

# ---------------------------------------------------------------------------
# Create or find site
# ---------------------------------------------------------------------------

if (-not $SiteName) {
    Write-Host ""
    $SiteName = Read-Host "Enter site name (e.g., Production, DR-Site)"
    if (-not $SiteName) { Write-Status "Site name required" "ERROR"; exit 1 }
}

$siteCode = ($SiteName -replace '[^a-zA-Z0-9]', '-').ToUpper()
if ($siteCode.Length -gt 10) { $siteCode = $siteCode.Substring(0, 10) }

Write-Status "Looking for site '$SiteName'..." "INFO"

$sitesResp = Invoke-Api -Method GET -Path "/sites" -Token $token
$sitesList = if ($sitesResp.sites) { $sitesResp.sites } elseif ($sitesResp -is [array]) { $sitesResp } else { @() }
$existingSite = $sitesList | Where-Object { $_.name -eq $SiteName } | Select-Object -First 1

if ($existingSite) {
    $siteId = $existingSite.id
    Write-Status "Site '$SiteName' exists (ID: $siteId)" "INFO"
} else {
    Write-Status "Creating site '$SiteName'..." "INFO"
    $site = Invoke-Api -Method POST -Path "/sites" -Token $token -Body @{
        name      = $SiteName
        code      = $siteCode
        site_type = "primary"
    }
    $siteId = $site.id
    Write-Status "Site created (ID: $siteId)" "SUCCESS"
}

# ---------------------------------------------------------------------------
# Check if gateway exists for this site
# ---------------------------------------------------------------------------

Write-Status "Checking gateways for site '$SiteName'..." "INFO"

$gwResp = Invoke-Api -Method GET -Path "/gateways" -Token $token
$gwList = if ($gwResp.gateways) { $gwResp.gateways } elseif ($gwResp -is [array]) { $gwResp } else { @() }
# Flatten grouped response (gateways grouped by site)
if ($gwList.Count -gt 0 -and $gwList[0].gateways) {
    $flatGw = @()
    foreach ($group in $gwList) {
        if ($group.gateways) { $flatGw += $group.gateways }
    }
    $gwList = $flatGw
}

$siteGateway = $gwList | Where-Object { $_.site_id -eq $siteId } | Select-Object -First 1
$gwAlreadyExists = $null -ne $siteGateway

if ($gwAlreadyExists) {
    Write-Status "Gateway already registered: $($siteGateway.name) (ID: $($siteGateway.id))" "INFO"
} else {
    Write-Status "No gateway for this site — will enroll one" "INFO"
}

# ---------------------------------------------------------------------------
# Check if agent exists
# ---------------------------------------------------------------------------

Write-Status "Checking agents..." "INFO"

$agentsResp = Invoke-Api -Method GET -Path "/agents" -Token $token
$agentsList = if ($agentsResp.agents) { $agentsResp.agents } elseif ($agentsResp -is [array]) { $agentsResp } else { @() }

$hostname = [System.Net.Dns]::GetHostName()
$myAgent = $agentsList | Where-Object { $_.hostname -eq $hostname } | Select-Object -First 1
$agentAlreadyExists = $null -ne $myAgent

if ($agentAlreadyExists) {
    Write-Status "Agent already registered: $hostname (ID: $($myAgent.id))" "INFO"
} else {
    Write-Status "No agent for hostname '$hostname' — will enroll one" "INFO"
}

# ---------------------------------------------------------------------------
# Start / Enroll gateway
# ---------------------------------------------------------------------------

Write-Host ""
Write-Status "Starting gateway..." "INFO"

if (-not $gwAlreadyExists) {
    # Create enrollment token for gateway
    $gwTokenResp = Invoke-Api -Method POST -Path "/enrollment/tokens" -Token $token -Body @{
        name        = "GW-$SiteName-$(Get-Date -Format 'yyyyMMdd')"
        scope       = "gateway"
        max_uses    = 1
        valid_hours = 8760
    }
    $gwEnrollToken = $gwTokenResp.token
    Write-Status "Gateway enrollment token created" "SUCCESS"

    $env:GATEWAY_ENROLLMENT_TOKEN = $gwEnrollToken
} else {
    # Already enrolled — no token needed (gateway remembers its cert)
    $env:GATEWAY_ENROLLMENT_TOKEN = ""
}

$env:BACKEND_URL = "ws://$($BackendUrl -replace 'http://','' -replace 'https://','')//ws/gateway"
$env:GATEWAY_ZONE = $siteCode
$env:GATEWAY_LISTEN_PORT = "$GatewayPort"
$env:RUST_LOG = "info"

$gwProcess = Start-Process -FilePath $gwBin -PassThru -WindowStyle Normal
Start-Sleep -Seconds 5
Write-Status "Gateway started (PID: $($gwProcess.Id))" "SUCCESS"

# Assign gateway to site if newly enrolled
if (-not $gwAlreadyExists) {
    Start-Sleep -Seconds 2
    Write-Status "Assigning gateway to site..." "INFO"

    # Re-fetch gateways to find the newly registered one
    $gwResp2 = Invoke-Api -Method GET -Path "/gateways" -Token $token
    $gwList2 = if ($gwResp2.gateways) { $gwResp2.gateways } elseif ($gwResp2 -is [array]) { $gwResp2 } else { @() }
    if ($gwList2.Count -gt 0 -and $gwList2[0].gateways) {
        $flatGw2 = @()
        foreach ($group in $gwList2) { if ($group.gateways) { $flatGw2 += $group.gateways } }
        $gwList2 = $flatGw2
    }

    $newGw = $gwList2 | Where-Object { -not $_.site_id -or $_.site_id -eq $null } | Sort-Object created_at -Descending | Select-Object -First 1
    if (-not $newGw) {
        $newGw = $gwList2 | Sort-Object created_at -Descending | Select-Object -First 1
    }

    if ($newGw) {
        try {
            Invoke-Api -Method PUT -Path "/gateways/$($newGw.id)" -Token $token -Body @{ site_id = $siteId }
            Write-Status "Gateway assigned to site '$SiteName'" "SUCCESS"
        } catch {
            Write-Status "Could not assign gateway (may need manual assignment in UI)" "WARNING"
        }
    }
}

# ---------------------------------------------------------------------------
# Start / Enroll agent
# ---------------------------------------------------------------------------

Write-Host ""
Write-Status "Starting agent..." "INFO"

if (-not $agentAlreadyExists) {
    # Create enrollment token for agent
    $agentTokenResp = Invoke-Api -Method POST -Path "/enrollment/tokens" -Token $token -Body @{
        name        = "Agent-$hostname-$(Get-Date -Format 'yyyyMMdd')"
        scope       = "agent"
        max_uses    = 1
        valid_hours = 8760
    }
    $agentEnrollToken = $agentTokenResp.token
    Write-Status "Agent enrollment token created" "SUCCESS"

    $env:AGENT_ENROLLMENT_TOKEN = $agentEnrollToken
} else {
    $env:AGENT_ENROLLMENT_TOKEN = ""
}

$env:GATEWAY_URL = "ws://localhost:$GatewayPort"
$env:RUST_LOG = "info"

$agentProcess = Start-Process -FilePath $agentBin -PassThru -WindowStyle Normal
Start-Sleep -Seconds 3
Write-Status "Agent started (PID: $($agentProcess.Id))" "SUCCESS"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "  Site '$SiteName' is ready!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
Write-Host ""
Write-Host "  Site:      $SiteName (code: $siteCode)"
Write-Host "  Gateway:   PID $($gwProcess.Id) — port $GatewayPort"
Write-Host "  Agent:     PID $($agentProcess.Id) — hostname $hostname"
Write-Host "  Web UI:    $BackendUrl"
Write-Host ""
if (-not $gwAlreadyExists -or -not $agentAlreadyExists) {
    Write-Host "  First run: gateway and agent are being enrolled." -ForegroundColor Yellow
    Write-Host "  Next run:  they will reconnect automatically." -ForegroundColor Yellow
} else {
    Write-Host "  Reconnected existing gateway + agent." -ForegroundColor Cyan
}
Write-Host ""
Write-Host "  To add another site:" -ForegroundColor Yellow
Write-Host "    .\setup-site.ps1 -SiteName 'DR-Site' -GatewayPort 4444" -ForegroundColor Yellow
Write-Host ""
