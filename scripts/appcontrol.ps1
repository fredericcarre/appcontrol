# appcontrol.ps1 - Unified cross-platform AppControl standalone deployment script
# Works on Windows PowerShell 5.1+ and PowerShell Core 6+

param(
    [Parameter(Position=0)]
    [string]$Command,
    [Parameter(Position=1)]
    [string]$Arg1,
    [Parameter(Position=2)]
    [string]$Arg2,
    [string]$Email = "admin@localhost",
    [string]$Password = "admin",
    [string]$BackendPort = "3000"
)

$ErrorActionPreference = "Stop"

# --- Platform detection ---
if ($PSVersionTable.PSVersion.Major -ge 6) {
    $script:IsWin = $IsWindows
} else {
    $script:IsWin = $true
}

$script:BinExt = ""
if ($script:IsWin) { $script:BinExt = ".exe" }

$script:PlatformSuffix = "linux-amd64"
if ($script:IsWin) { $script:PlatformSuffix = "windows-amd64" }

# --- Directories ---
$script:ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$script:BinDir = Join-Path $script:ScriptDir "bin"
$script:DataDir = Join-Path $script:ScriptDir "data"
$script:ConfigDir = Join-Path $script:ScriptDir "config"
$script:LogDir = Join-Path $script:ScriptDir "logs"
$script:FrontendDir = Join-Path $script:ScriptDir "frontend"

$script:SettingsFile = Join-Path $script:ConfigDir "settings.json"
$script:SitesFile = Join-Path $script:ConfigDir "sites.json"
$script:PidsFile = Join-Path $script:DataDir "pids.json"

$script:ReleasesBase = "https://github.com/fredericcarre/appcontrol/releases/latest/download"

# ---------------------------------------------------------------------------
# Utility functions
# ---------------------------------------------------------------------------

function Write-Info { param([string]$Msg) Write-Host ("[INFO] " + $Msg) -ForegroundColor Cyan }
function Write-Ok   { param([string]$Msg) Write-Host ("[OK]   " + $Msg) -ForegroundColor Green }
function Write-Warn { param([string]$Msg) Write-Host ("[WARN] " + $Msg) -ForegroundColor Yellow }
function Write-Err  { param([string]$Msg) Write-Host ("[ERR]  " + $Msg) -ForegroundColor Red }

function Ensure-Dir {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
    }
}

function Generate-Secret {
    $bytes = New-Object byte[] 32
    $rng = New-Object System.Security.Cryptography.RNGCryptoServiceProvider
    $rng.GetBytes($bytes)
    $rng.Dispose()
    return [Convert]::ToBase64String($bytes)
}

function Read-Settings {
    if (-not (Test-Path $script:SettingsFile)) { return $null }
    $raw = Get-Content $script:SettingsFile -Raw
    return ($raw | ConvertFrom-Json)
}

function Write-Settings {
    param([object]$Settings)
    $json = $Settings | ConvertTo-Json -Depth 10
    Set-Content -Path $script:SettingsFile -Value $json -Encoding UTF8
}

function Read-Sites {
    if (-not (Test-Path $script:SitesFile)) { return @() }
    $raw = Get-Content $script:SitesFile -Raw
    if (-not $raw -or $raw.Trim() -eq "") { return @() }
    $parsed = $raw | ConvertFrom-Json
    if ($parsed -is [array]) { return $parsed }
    return @($parsed)
}

function Write-Sites {
    param([object]$Sites)
    $json = ConvertTo-Json -InputObject @($Sites) -Depth 10
    Set-Content -Path $script:SitesFile -Value $json -Encoding UTF8
}

function Read-Pids {
    if (-not (Test-Path $script:PidsFile)) { return $null }
    $raw = Get-Content $script:PidsFile -Raw
    if (-not $raw -or $raw.Trim() -eq "") { return $null }
    return ($raw | ConvertFrom-Json)
}

function Write-Pids {
    param([object]$Pids)
    $json = $Pids | ConvertTo-Json -Depth 10
    Set-Content -Path $script:PidsFile -Value $json -Encoding UTF8
}

function Is-ProcessRunning {
    param([int]$ProcessId)
    try {
        $proc = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
        if ($proc -and (-not $proc.HasExited)) { return $true }
        return $false
    } catch {
        return $false
    }
}

function Invoke-Api {
    param(
        [string]$Method = "GET",
        [string]$Uri,
        [object]$Body,
        [string]$Token
    )
    $request = [System.Net.HttpWebRequest]::Create($Uri)
    $request.Method = $Method.ToUpper()
    $request.Accept = "application/json"
    if ($Token) {
        $request.Headers.Add("Authorization", "Bearer " + $Token)
    }
    if ($Body) {
        $request.ContentType = "application/json; charset=utf-8"
        $jsonBody = ($Body | ConvertTo-Json -Depth 10)
        $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($jsonBody)
        $request.ContentLength = $bodyBytes.Length
        $reqStream = $request.GetRequestStream()
        $reqStream.Write($bodyBytes, 0, $bodyBytes.Length)
        $reqStream.Close()
    }
    try {
        $response = $request.GetResponse()
        $reader = New-Object System.IO.StreamReader($response.GetResponseStream(), [System.Text.Encoding]::UTF8)
        $responseText = $reader.ReadToEnd()
        $reader.Close()
        $response.Close()
        if ($responseText) { return ($responseText | ConvertFrom-Json) }
        return $null
    } catch [System.Net.WebException] {
        $webEx = $_.Exception
        if ($webEx.Response) {
            $status = [int]$webEx.Response.StatusCode
            if ($status -eq 409) { return $null }
            if ($status -eq 404) { return $null }
        }
        throw
    }
}

function Login-Backend {
    param([string]$Port, [string]$AdminEmail, [string]$AdminPassword)
    $uri = "http://localhost:" + $Port + "/api/v1/auth/login"
    $body = @{ email = $AdminEmail; password = $AdminPassword }
    $result = Invoke-Api -Method "POST" -Uri $uri -Body $body
    if ($result -and $result.token) { return $result.token }
    if ($result -and $result.access_token) { return $result.access_token }
    throw "Login failed - no token returned"
}

function Download-File {
    param([string]$Url, [string]$OutPath)
    Write-Info ("Downloading " + $Url)
    Invoke-WebRequest -Uri $Url -OutFile $OutPath -UseBasicParsing
    # Mark executable on Linux
    if (-not $script:IsWin) {
        if ($OutPath -notlike "*.zip") {
            & chmod +x $OutPath
        }
    }
}

# ---------------------------------------------------------------------------
# INSTALL
# ---------------------------------------------------------------------------
function Do-Install {
    Write-Info "Installing AppControl standalone..."

    Ensure-Dir $script:BinDir
    Ensure-Dir $script:DataDir
    Ensure-Dir $script:ConfigDir
    Ensure-Dir $script:LogDir
    Ensure-Dir $script:FrontendDir

    # Generate settings if not present
    if (-not (Test-Path $script:SettingsFile)) {
        $secret = Generate-Secret
        $settings = @{
            jwt_secret     = $secret
            admin_email    = $Email
            admin_password = $Password
            org_name       = "AppControl"
            backend_port   = $BackendPort
        }
        Write-Settings $settings
        Write-Ok "Generated config/settings.json"
    } else {
        Write-Info "config/settings.json already exists, keeping it"
    }

    # Download binaries
    $binaries = @("appcontrol-backend-sqlite", "appcontrol-gateway", "appcontrol-agent")
    foreach ($bin in $binaries) {
        $fileName = $bin + "-" + $script:PlatformSuffix
        if ($script:IsWin -and (-not $fileName.EndsWith(".exe"))) {
            $fileName = $fileName + ".exe"
        }
        $localName = $bin + $script:BinExt
        $url = $script:ReleasesBase + "/" + $fileName
        $outPath = Join-Path $script:BinDir $localName
        Download-File -Url $url -OutPath $outPath
    }
    Write-Ok "Binaries downloaded to bin/"

    # Download frontend (optional - may not be available in all releases)
    $frontendZip = Join-Path $script:BinDir "frontend.zip"
    $frontendUrl = $script:ReleasesBase + "/appcontrol-frontend.zip"
    try {
        Download-File -Url $frontendUrl -OutPath $frontendZip
        if (Test-Path $script:FrontendDir) {
            Remove-Item -Path (Join-Path $script:FrontendDir "*") -Recurse -Force -ErrorAction SilentlyContinue
        }
        Expand-Archive -Path $frontendZip -DestinationPath $script:FrontendDir -Force
        Remove-Item $frontendZip -Force -ErrorAction SilentlyContinue
        Write-Ok "Frontend extracted to frontend/"
    } catch {
        Write-Warn "Frontend not available for download (not required for API-only mode)"
    }

    # Create empty sites.json if missing
    if (-not (Test-Path $script:SitesFile)) {
        Set-Content -Path $script:SitesFile -Value "[]" -Encoding UTF8
    }

    # Create start.bat on Windows
    if ($script:IsWin) {
        $batPath = Join-Path $script:ScriptDir "start.bat"
        $batContent = "@echo off" + "`r`n" + "powershell -ExecutionPolicy Bypass -File " + """" + (Join-Path $script:ScriptDir "appcontrol.ps1") + """" + " start"
        Set-Content -Path $batPath -Value $batContent -Encoding ASCII
        Write-Ok "Created start.bat"
    }

    Write-Ok "Installation complete. Run: .\appcontrol.ps1 start"
}

# ---------------------------------------------------------------------------
# START
# ---------------------------------------------------------------------------
function Do-Start {
    Write-Info "Starting AppControl..."

    $settings = Read-Settings
    if (-not $settings) {
        Write-Err "No config/settings.json found. Run install first."
        return
    }

    $port = $settings.backend_port
    if (-not $port) { $port = "3000" }

    $backendBin = Join-Path $script:BinDir ("appcontrol-backend-sqlite" + $script:BinExt)
    if (-not (Test-Path $backendBin)) {
        Write-Err ("Backend binary not found: " + $backendBin)
        return
    }

    # Build environment
    $dbPath = Join-Path $script:DataDir "appcontrol.db"
    $env:DATABASE_URL = "sqlite:" + $dbPath
    $env:JWT_SECRET = $settings.jwt_secret
    $env:LOCAL_AUTH_ENABLED = "true"
    $env:SEED_ENABLED = "true"
    $env:SEED_ADMIN_EMAIL = $settings.admin_email
    $env:SEED_ADMIN_PASSWORD = $settings.admin_password
    $env:SEED_ORG_NAME = "AppControl"
    $env:SEED_ORG_SLUG = "appcontrol"
    $env:APP_ENV = "development"
    $env:RUST_LOG = "info"
    $env:STATIC_DIR = $script:FrontendDir
    $env:PORT = $port

    # Start backend
    $backendLog = Join-Path $script:LogDir "backend.log"
    $backendErr = Join-Path $script:LogDir "backend.err.log"
    $backendProc = Start-Process -FilePath $backendBin -PassThru -NoNewWindow `
        -RedirectStandardOutput $backendLog -RedirectStandardError $backendErr
    Write-Info ("Backend started with PID " + $backendProc.Id)

    # Poll health
    $healthUrl = "http://localhost:" + $port + "/health"
    $healthy = $false
    for ($i = 0; $i -lt 30; $i++) {
        Start-Sleep -Seconds 1
        try {
            $resp = Invoke-WebRequest -Uri $healthUrl -UseBasicParsing -TimeoutSec 2
            if ($resp.StatusCode -eq 200) {
                $healthy = $true
                break
            }
        } catch {
            # not ready yet
        }
    }
    if (-not $healthy) {
        Write-Err "Backend did not become healthy within 30 seconds. Check logs/backend.err.log"
        return
    }
    Write-Ok "Backend is healthy"

    # Prepare pids object
    $pids = @{
        backend  = $backendProc.Id
        gateways = @{}
        agents   = @{}
    }

    # Start gateways and agents for each site
    $sites = Read-Sites
    foreach ($site in $sites) {
        $siteName = $site.name
        $gwPort = $site.gateway_port
        if (-not $gwPort) { $gwPort = 4443 }

        # Gateway
        $gwBin = Join-Path $script:BinDir ("appcontrol-gateway" + $script:BinExt)
        if (Test-Path $gwBin) {
            $gwId = $site.gateway_id
            if (-not $gwId) { $gwId = "gw-" + ($siteName -replace '[^a-zA-Z0-9]', '-').ToLower() }
            $env:GATEWAY_ID = $gwId
            $env:BACKEND_URL = "ws://localhost:" + $port + "/ws/gateway"
            $env:LISTEN_PORT = [string]$gwPort
            $env:GATEWAY_ZONE = $siteName
            $env:GATEWAY_NAME = ("gw-" + $siteName)
            if ($site.site_id) { $env:GATEWAY_SITE_ID = $site.site_id }
            if ($site.gw_enroll_token) { $env:GATEWAY_ENROLLMENT_TOKEN = $site.gw_enroll_token }

            $gwLog = Join-Path $script:LogDir ("gateway-" + $siteName + ".log")
            $gwErr = Join-Path $script:LogDir ("gateway-" + $siteName + ".err.log")
            $gwProc = Start-Process -FilePath $gwBin -PassThru -NoNewWindow `
                -RedirectStandardOutput $gwLog -RedirectStandardError $gwErr
            $pids.gateways[$siteName] = $gwProc.Id
            Write-Info ("Gateway '" + $siteName + "' started with PID " + $gwProc.Id + " on port " + $gwPort)

            # Wait for gateway to be ready before starting agent
            Start-Sleep -Seconds 3
        }

        # Agent (uses enrolled config from data/agent-<sitename>/agent.yaml)
        $agBin = Join-Path $script:BinDir ("appcontrol-agent" + $script:BinExt)
        if (Test-Path $agBin) {
            $agDataDir = Join-Path $script:DataDir ("agent-" + $siteName)
            $agConfigFile = Join-Path $agDataDir "agent.yaml"
            $agLog = Join-Path $script:LogDir ("agent-" + $siteName + ".log")
            $agErr = Join-Path $script:LogDir ("agent-" + $siteName + ".err.log")

            if (Test-Path $agConfigFile) {
                # Enrolled agent — use its config file
                $agArgs = ("--config " + """" + $agConfigFile + """")
                $agProc = Start-Process -FilePath $agBin -ArgumentList $agArgs -PassThru -NoNewWindow `
                    -RedirectStandardOutput $agLog -RedirectStandardError $agErr
            } else {
                # Fallback: no enrollment yet — connect directly
                Write-Warn ("Agent for '" + $siteName + "' not enrolled. Run add-site again to enroll.")
                $env:GATEWAY_URL = "ws://localhost:" + $gwPort
                Ensure-Dir $agDataDir
                $env:DATA_DIR = $agDataDir
                $agProc = Start-Process -FilePath $agBin -PassThru -NoNewWindow `
                    -RedirectStandardOutput $agLog -RedirectStandardError $agErr
            }
            $pids.agents[$siteName] = $agProc.Id
            Write-Info ("Agent '" + $siteName + "' started with PID " + $agProc.Id)
        }

        Start-Sleep -Milliseconds 500
    }

    Write-Pids $pids

    Write-Host ""
    Write-Ok "AppControl is running"
    Write-Host ("  Backend:   http://localhost:" + $port)
    Write-Host ("  Frontend:  http://localhost:" + $port)
    Write-Host ("  Logs:      " + $script:LogDir)
    if ($sites.Count -gt 0) {
        foreach ($site in $sites) {
            Write-Host ("  Gateway '" + $site.name + "': port " + $site.gateway_port)
        }
    }
    Write-Host ""
}

# ---------------------------------------------------------------------------
# STOP
# ---------------------------------------------------------------------------
function Do-Stop {
    Write-Info "Stopping AppControl..."

    $pids = Read-Pids
    if (-not $pids) {
        Write-Warn "No data/pids.json found. Nothing to stop."
        Do-StopFallback
        return
    }

    # Stop agents first
    if ($pids.agents) {
        $agentProps = $pids.agents.PSObject.Properties
        foreach ($prop in $agentProps) {
            $name = $prop.Name
            $procId = [int]$prop.Value
            Write-Info ("Stopping agent '" + $name + "' (PID " + $procId + ")")
            Stop-Process -Id $procId -Force -ErrorAction SilentlyContinue
        }
    }

    # Stop gateways
    if ($pids.gateways) {
        $gwProps = $pids.gateways.PSObject.Properties
        foreach ($prop in $gwProps) {
            $name = $prop.Name
            $procId = [int]$prop.Value
            Write-Info ("Stopping gateway '" + $name + "' (PID " + $procId + ")")
            Stop-Process -Id $procId -Force -ErrorAction SilentlyContinue
        }
    }

    # Stop backend
    if ($pids.backend) {
        $procId = [int]$pids.backend
        Write-Info ("Stopping backend (PID " + $procId + ")")
        Stop-Process -Id $procId -Force -ErrorAction SilentlyContinue
    }

    # Remove pids file
    if (Test-Path $script:PidsFile) {
        Remove-Item $script:PidsFile -Force
    }

    Do-StopFallback
    Write-Ok "All processes stopped"
}

function Do-StopFallback {
    # Fallback: kill by process name
    $names = @("appcontrol-backend-sqlite", "appcontrol-gateway", "appcontrol-agent")
    foreach ($name in $names) {
        $procs = Get-Process -Name $name -ErrorAction SilentlyContinue
        if ($procs) {
            foreach ($p in $procs) {
                Write-Warn ("Fallback: stopping " + $name + " PID " + $p.Id)
                Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
            }
        }
    }
}

# ---------------------------------------------------------------------------
# STATUS
# ---------------------------------------------------------------------------
function Do-Status {
    Write-Host ""
    Write-Host "=== AppControl Status ===" -ForegroundColor Cyan
    Write-Host ""

    $pids = Read-Pids
    if (-not $pids) {
        Write-Warn "No data/pids.json - AppControl may not be running"
        Write-Host ""
        return
    }

    # Backend
    $bPid = 0
    if ($pids.backend) { $bPid = [int]$pids.backend }
    $bStatus = "STOPPED"
    if ($bPid -gt 0 -and (Is-ProcessRunning $bPid)) { $bStatus = "RUNNING" }
    $bColor = "Red"
    if ($bStatus -eq "RUNNING") { $bColor = "Green" }
    Write-Host ("  Backend       PID " + $bPid + "   ") -NoNewline
    Write-Host $bStatus -ForegroundColor $bColor

    # Gateways
    if ($pids.gateways) {
        $gwProps = $pids.gateways.PSObject.Properties
        foreach ($prop in $gwProps) {
            $name = $prop.Name
            $procId = [int]$prop.Value
            $status = "STOPPED"
            if ($procId -gt 0 -and (Is-ProcessRunning $procId)) { $status = "RUNNING" }
            $color = "Red"
            if ($status -eq "RUNNING") { $color = "Green" }
            Write-Host ("  Gateway       " + $name + "  PID " + $procId + "   ") -NoNewline
            Write-Host $status -ForegroundColor $color
        }
    }

    # Agents
    if ($pids.agents) {
        $agProps = $pids.agents.PSObject.Properties
        foreach ($prop in $agProps) {
            $name = $prop.Name
            $procId = [int]$prop.Value
            $status = "STOPPED"
            if ($procId -gt 0 -and (Is-ProcessRunning $procId)) { $status = "RUNNING" }
            $color = "Red"
            if ($status -eq "RUNNING") { $color = "Green" }
            Write-Host ("  Agent         " + $name + "  PID " + $procId + "   ") -NoNewline
            Write-Host $status -ForegroundColor $color
        }
    }

    # Database info
    $dbPath = Join-Path $script:DataDir "appcontrol.db"
    if (Test-Path $dbPath) {
        $dbSize = (Get-Item $dbPath).Length
        $dbSizeMB = [math]::Round($dbSize / 1MB, 2)
        Write-Host ""
        Write-Host ("  Database:     " + $dbPath + " (" + $dbSizeMB + " MB)")
    }

    Write-Host ""
}

# ---------------------------------------------------------------------------
# ADD-SITE
# ---------------------------------------------------------------------------
function Do-AddSite {
    $siteName = $Arg1
    if (-not $siteName) {
        Write-Err "Usage: appcontrol.ps1 add-site <name> [gateway-port]"
        return
    }

    $gwPort = 4443
    if ($Arg2) {
        $gwPort = [int]$Arg2
    } else {
        # Auto-increment port to avoid conflicts with existing sites
        $existingSites = Read-Sites
        foreach ($s in $existingSites) {
            if ($s.gateway_port -and [int]$s.gateway_port -ge $gwPort) {
                $gwPort = [int]$s.gateway_port + 1
            }
        }
    }

    $settings = Read-Settings
    if (-not $settings) {
        Write-Err "No config/settings.json found. Run install first."
        return
    }

    $port = $settings.backend_port
    if (-not $port) { $port = "3000" }

    Write-Info ("Adding site '" + $siteName + "' on gateway port " + $gwPort)

    # Login
    $adminEmail = $settings.admin_email
    $adminPass = $settings.admin_password
    Write-Info "Logging in to backend..."
    $token = Login-Backend -Port $port -AdminEmail $adminEmail -AdminPassword $adminPass
    Write-Ok "Authenticated"

    # Check existing sites
    $baseUrl = "http://localhost:" + $port + "/api/v1"
    $existingSites = Invoke-Api -Method "GET" -Uri ($baseUrl + "/sites") -Token $token
    $siteId = $null

    if ($existingSites) {
        $siteList = $existingSites
        if ($existingSites.data) { $siteList = $existingSites.data }
        if ($existingSites.sites) { $siteList = $existingSites.sites }
        foreach ($s in $siteList) {
            if ($s.name -eq $siteName) {
                $siteId = $s.id
                Write-Info ("Site '" + $siteName + "' already exists with ID " + $siteId)
                break
            }
        }
    }

    # Create site if needed
    if (-not $siteId) {
        $siteCode = ($siteName -replace '[^a-zA-Z0-9]', '-').ToUpper()
        if ($siteCode.Length -gt 10) { $siteCode = $siteCode.Substring(0, 10) }
        $newSiteBody = @{
            name      = $siteName
            code      = $siteCode
            site_type = "primary"
        }
        $created = Invoke-Api -Method "POST" -Uri ($baseUrl + "/sites") -Body $newSiteBody -Token $token
        if ($created -and $created.id) {
            $siteId = $created.id
        } elseif ($created -and $created.data -and $created.data.id) {
            $siteId = $created.data.id
        }
        if ($siteId) {
            Write-Ok ("Created site '" + $siteName + "' with ID " + $siteId)
        } else {
            Write-Warn "Could not determine site ID after creation"
            $siteId = "unknown"
        }
    }

    # --- PKI initialization (once) ---
    Write-Info "Checking PKI..."
    $caResult = $null
    try {
        $caResult = Invoke-Api -Method "GET" -Uri ($baseUrl + "/enrollment/pki/ca") -Token $token
    } catch {}
    if (-not $caResult -or -not $caResult.ca_cert_pem) {
        Write-Info "Initializing PKI (Certificate Authority)..."
        $pkiBody = @{ org_name = "AppControl"; validity_days = 3650 }
        try {
            Invoke-Api -Method "POST" -Uri ($baseUrl + "/enrollment/pki/init") -Body $pkiBody -Token $token | Out-Null
            Write-Ok "PKI initialized"
        } catch {
            Write-Warn "PKI init failed (may already exist) - continuing"
        }
    } else {
        Write-Ok "PKI already initialized"
    }

    # --- Create gateway enrollment token ---
    Write-Info "Creating gateway enrollment token..."
    $gwTokenBody = @{
        name       = ("gw-enroll-" + $siteName)
        scope      = "gateway"
        max_uses   = 10
        valid_hours = 8760
    }
    $gwTokenResult = Invoke-Api -Method "POST" -Uri ($baseUrl + "/enrollment/tokens") -Body $gwTokenBody -Token $token
    $gwEnrollToken = $null
    if ($gwTokenResult -and $gwTokenResult.token) {
        $gwEnrollToken = $gwTokenResult.token
        Write-Ok "Gateway enrollment token created"
    } else {
        Write-Err "Failed to create gateway enrollment token"
        return
    }

    # --- Create agent enrollment token ---
    Write-Info "Creating agent enrollment token..."
    $agTokenBody = @{
        name       = ("agent-enroll-" + $siteName)
        scope      = "agent"
        max_uses   = 10
        valid_hours = 8760
    }
    $agTokenResult = Invoke-Api -Method "POST" -Uri ($baseUrl + "/enrollment/tokens") -Body $agTokenBody -Token $token
    $agEnrollToken = $null
    if ($agTokenResult -and $agTokenResult.token) {
        $agEnrollToken = $agTokenResult.token
        Write-Ok "Agent enrollment token created"
    } else {
        Write-Err "Failed to create agent enrollment token"
        return
    }

    # Save to sites.json
    $existingSites = Read-Sites
    $sitesList = New-Object System.Collections.ArrayList
    foreach ($s in $existingSites) { $sitesList.Add($s) | Out-Null }
    $found = $false
    foreach ($s in $sitesList) {
        if ($s.name -eq $siteName) {
            $found = $true
            break
        }
    }
    if (-not $found) {
        $gwId = "gw-" + ($siteName -replace '[^a-zA-Z0-9]', '-').ToLower()
        $newEntry = @{
            name               = $siteName
            site_id            = $siteId
            gateway_port       = $gwPort
            gateway_id         = $gwId
            gw_enroll_token    = $gwEnrollToken
            agent_enroll_token = $agEnrollToken
        }
        $sitesList.Add($newEntry) | Out-Null
        Write-Sites $sitesList
        Write-Ok "Saved site to config/sites.json"
    }

    # If backend is running, start gateway + agent for this site
    $pids = Read-Pids
    if ($pids -and $pids.backend) {
        $bPid = [int]$pids.backend
        if (Is-ProcessRunning $bPid) {
            Write-Info "Backend is running, starting gateway and agent for new site..."

            # --- Start gateway with enrollment token ---
            $gwId = "gw-" + ($siteName -replace '[^a-zA-Z0-9]', '-').ToLower()
            $env:GATEWAY_ID = $gwId
            $env:BACKEND_URL = "ws://localhost:" + $port + "/ws/gateway"
            $env:LISTEN_PORT = [string]$gwPort
            $env:GATEWAY_ZONE = $siteName
            $env:GATEWAY_NAME = ("gw-" + $siteName)
            $env:GATEWAY_SITE_ID = $siteId
            $env:GATEWAY_ENROLLMENT_TOKEN = $gwEnrollToken

            $gwBin = Join-Path $script:BinDir ("appcontrol-gateway" + $script:BinExt)
            $gwLog = Join-Path $script:LogDir ("gateway-" + $siteName + ".log")
            $gwErr = Join-Path $script:LogDir ("gateway-" + $siteName + ".err.log")
            $gwProc = Start-Process -FilePath $gwBin -PassThru -NoNewWindow `
                -RedirectStandardOutput $gwLog -RedirectStandardError $gwErr
            Write-Info ("Gateway started with PID " + $gwProc.Id + " on port " + $gwPort)

            # Wait for gateway to be ready
            Start-Sleep -Seconds 5

            # --- Enroll agent via gateway ---
            $agBin = Join-Path $script:BinDir ("appcontrol-agent" + $script:BinExt)
            $agDataDir = Join-Path $script:DataDir ("agent-" + $siteName)
            Ensure-Dir $agDataDir
            $agEnrollUrl = "ws://localhost:" + $gwPort
            Write-Info ("Enrolling agent for site '" + $siteName + "'...")
            $enrollArgs = ("--enroll " + $agEnrollUrl + " --token " + $agEnrollToken + " --enroll-dir " + """" + $agDataDir + """")
            $agEnrollLog = Join-Path $script:LogDir ("agent-enroll-" + $siteName + ".log")
            $agEnrollErr = Join-Path $script:LogDir ("agent-enroll-" + $siteName + ".err.log")
            $enrollProc = Start-Process -FilePath $agBin -ArgumentList $enrollArgs -PassThru -NoNewWindow `
                -RedirectStandardOutput $agEnrollLog -RedirectStandardError $agEnrollErr
            $enrollProc.WaitForExit(30000) | Out-Null
            if ($enrollProc.ExitCode -eq 0) {
                Write-Ok "Agent enrolled successfully"
            } else {
                Write-Warn ("Agent enrollment exited with code " + $enrollProc.ExitCode + " - check " + $agEnrollErr)
            }

            # --- Start agent with enrolled config ---
            $agConfigFile = Join-Path $agDataDir "agent.yaml"
            $agArgs = ("--config " + """" + $agConfigFile + """")
            $agLog = Join-Path $script:LogDir ("agent-" + $siteName + ".log")
            $agErr = Join-Path $script:LogDir ("agent-" + $siteName + ".err.log")
            $agProc = Start-Process -FilePath $agBin -ArgumentList $agArgs -PassThru -NoNewWindow `
                -RedirectStandardOutput $agLog -RedirectStandardError $agErr
            Write-Info ("Agent started with PID " + $agProc.Id)

            # Update pids
            if (-not $pids.gateways) {
                $pids | Add-Member -NotePropertyName "gateways" -NotePropertyValue @{} -Force
            }
            if (-not $pids.agents) {
                $pids | Add-Member -NotePropertyName "agents" -NotePropertyValue @{} -Force
            }
            $pids.gateways | Add-Member -NotePropertyName $siteName -NotePropertyValue $gwProc.Id -Force
            $pids.agents | Add-Member -NotePropertyName $siteName -NotePropertyValue $agProc.Id -Force
            Write-Pids $pids

            Write-Ok ("Gateway PID " + $gwProc.Id + ", Agent PID " + $agProc.Id)
        }
    }

    Write-Ok ("Site '" + $siteName + "' added successfully")
}

# ---------------------------------------------------------------------------
# UPGRADE
# ---------------------------------------------------------------------------
function Do-Upgrade {
    Write-Info "Upgrading AppControl..."

    # Stop everything
    Do-Stop

    # Re-download binaries
    $binaries = @("appcontrol-backend-sqlite", "appcontrol-gateway", "appcontrol-agent")
    foreach ($bin in $binaries) {
        $fileName = $bin + "-" + $script:PlatformSuffix
        if ($script:IsWin -and (-not $fileName.EndsWith(".exe"))) {
            $fileName = $fileName + ".exe"
        }
        $localName = $bin + $script:BinExt
        $url = $script:ReleasesBase + "/" + $fileName
        $outPath = Join-Path $script:BinDir $localName
        Download-File -Url $url -OutPath $outPath
    }
    Write-Ok "Binaries updated"

    # Re-download frontend (optional)
    $frontendZip = Join-Path $script:BinDir "frontend.zip"
    $frontendUrl = $script:ReleasesBase + "/appcontrol-frontend.zip"
    try {
        Download-File -Url $frontendUrl -OutPath $frontendZip
        if (Test-Path $script:FrontendDir) {
            Remove-Item -Path (Join-Path $script:FrontendDir "*") -Recurse -Force -ErrorAction SilentlyContinue
        }
        Expand-Archive -Path $frontendZip -DestinationPath $script:FrontendDir -Force
        Remove-Item $frontendZip -Force -ErrorAction SilentlyContinue
        Write-Ok "Frontend updated"
    } catch {
        Write-Warn "Frontend not available for download"
    }

    # Restart
    Do-Start

    Write-Ok "Upgrade complete"
}

# ---------------------------------------------------------------------------
# LOGS
# ---------------------------------------------------------------------------
function Do-Logs {
    $logFile = $null
    if ($Arg1) {
        $logFile = Join-Path $script:LogDir $Arg1
        if (-not (Test-Path $logFile)) {
            # Try with .log extension
            $logFile = Join-Path $script:LogDir ($Arg1 + ".log")
        }
    } else {
        $logFile = Join-Path $script:LogDir "backend.log"
    }

    if (-not (Test-Path $logFile)) {
        Write-Err ("Log file not found: " + $logFile)
        Write-Info "Available logs:"
        $logs = Get-ChildItem -Path $script:LogDir -Filter "*.log" -ErrorAction SilentlyContinue
        foreach ($l in $logs) {
            Write-Host ("  " + $l.Name)
        }
        return
    }

    Write-Host ("=== Last 50 lines of " + $logFile + " ===") -ForegroundColor Cyan
    Get-Content $logFile -Tail 50
}

# ---------------------------------------------------------------------------
# HELP
# ---------------------------------------------------------------------------
function Do-Help {
    Write-Host ""
    Write-Host "AppControl Standalone Deployment" -ForegroundColor Cyan
    Write-Host "================================" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Usage: appcontrol.ps1 <command> [args]"
    Write-Host ""
    Write-Host "Commands:" -ForegroundColor Yellow
    Write-Host "  install                 Download binaries and set up directories"
    Write-Host "  start                   Start backend, gateways, and agents"
    Write-Host "  stop                    Stop all running processes"
    Write-Host "  status                  Show status of all components"
    Write-Host "  add-site <name> [port]  Add a new site (default gateway port: 4443)"
    Write-Host "  upgrade                 Stop, update binaries+frontend, restart"
    Write-Host "  logs [file]             Show recent log output"
    Write-Host "  help                    Show this help message"
    Write-Host ""
    Write-Host "Options:" -ForegroundColor Yellow
    Write-Host "  -Email <email>          Admin email (default: admin@localhost)"
    Write-Host "  -Password <pass>        Admin password (default: admin)"
    Write-Host "  -BackendPort <port>     Backend port (default: 3000)"
    Write-Host ""
    Write-Host "Examples:" -ForegroundColor Yellow
    Write-Host "  .\appcontrol.ps1 install"
    Write-Host "  .\appcontrol.ps1 start"
    Write-Host "  .\appcontrol.ps1 add-site Production 4443"
    Write-Host "  .\appcontrol.ps1 add-site DR-Site 4444"
    Write-Host "  .\appcontrol.ps1 status"
    Write-Host "  .\appcontrol.ps1 logs gateway-Production.log"
    Write-Host "  .\appcontrol.ps1 stop"
    Write-Host "  .\appcontrol.ps1 upgrade"
    Write-Host ""
    Write-Host "Directory layout:" -ForegroundColor Yellow
    Write-Host "  bin/        Binaries (overwritten by upgrade)"
    Write-Host "  data/       Database + runtime state (preserved)"
    Write-Host "  config/     Settings + sites (preserved)"
    Write-Host "  logs/       Log files"
    Write-Host "  frontend/   Web UI static files (overwritten by upgrade)"
    Write-Host ""
}

# ---------------------------------------------------------------------------
# Main dispatch
# ---------------------------------------------------------------------------
$cmd = ""
if ($Command) { $cmd = $Command.ToLower() }

switch ($cmd) {
    "install"  { Do-Install }
    "start"    { Do-Start }
    "stop"     { Do-Stop }
    "status"   { Do-Status }
    "add-site" { Do-AddSite }
    "upgrade"  { Do-Upgrade }
    "logs"     { Do-Logs }
    "help"     { Do-Help }
    default    { Do-Help }
}
