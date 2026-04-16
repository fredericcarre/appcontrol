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

    # Kill any stale processes from a previous run to avoid port conflicts
    # and sled DB lock errors (WSAEADDRINUSE / "Le processus ne peut pas acceder au fichier")
    $staleNames = @("appcontrol-backend-sqlite", "appcontrol-backend", "appcontrol-gateway", "appcontrol-agent")
    $foundStale = $false
    foreach ($name in $staleNames) {
        $procs = Get-Process -Name $name -ErrorAction SilentlyContinue
        if ($procs) {
            $foundStale = $true
            foreach ($p in $procs) {
                Write-Warn ("Killing stale process: " + $name + " PID " + $p.Id)
                Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
            }
        }
    }
    if ($foundStale) {
        Write-Info "Waiting for stale processes to release ports and file locks..."
        Start-Sleep -Seconds 3
    }

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
            # Do NOT pass enrollment token on restart -- the token has limited uses
            # and would be rejected after a few stop/start cycles.
            # In dev mode (single org / SQLite), the backend accepts gateways without
            # a token, so this is safe for local setups.
            $env:GATEWAY_ENROLLMENT_TOKEN = ""

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

            # Set unique hostname per site so each agent gets a distinct ID on restart
            $realHostname = [System.Net.Dns]::GetHostName()
            $env:AGENT_HOSTNAME = ($realHostname + "-" + $siteName)

            if (Test-Path $agConfigFile) {
                # Enrolled agent -- use its config file
                $agArgList = @("--config", "`"$agConfigFile`"")
                $agProc = Start-Process -FilePath $agBin -ArgumentList $agArgList -PassThru -NoNewWindow `
                    -RedirectStandardOutput $agLog -RedirectStandardError $agErr
            } else {
                # Fallback: no enrollment yet -- connect directly
                Write-Warn ("Agent for '" + $siteName + "' not enrolled. Run add-site again to enroll.")
                $env:GATEWAY_URL = "wss://localhost:" + $gwPort
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

    # Wait for all processes to actually terminate and release ports/file locks
    Write-Info "Waiting for processes to terminate..."
    $maxWait = 10
    for ($i = 0; $i -lt $maxWait; $i++) {
        $names = @("appcontrol-backend-sqlite", "appcontrol-backend", "appcontrol-gateway", "appcontrol-agent")
        $stillRunning = $false
        foreach ($name in $names) {
            if (Get-Process -Name $name -ErrorAction SilentlyContinue) {
                $stillRunning = $true
                break
            }
        }
        if (-not $stillRunning) { break }
        Start-Sleep -Seconds 1
    }
    if ($stillRunning) {
        Write-Warn "Some processes still running after ${maxWait}s -- force killing"
        Do-StopFallback
        Start-Sleep -Seconds 2
    }

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
            # Set unique hostname per site so each agent gets a distinct ID
            $realHostname = [System.Net.Dns]::GetHostName()
            $env:AGENT_HOSTNAME = ($realHostname + "-" + $siteName)
            $agEnrollUrl = "wss://localhost:" + $gwPort
            Write-Info ("Enrolling agent for site '" + $siteName + "' as " + $env:AGENT_HOSTNAME + "...")
            $enrollArgList = @("--enroll", $agEnrollUrl, "--token", $agEnrollToken, "--enroll-dir", "`"$agDataDir`"")
            $agEnrollLog = Join-Path $script:LogDir ("agent-enroll-" + $siteName + ".log")
            $agEnrollErr = Join-Path $script:LogDir ("agent-enroll-" + $siteName + ".err.log")
            Write-Info ("Enroll command: " + $agBin + " " + ($enrollArgList -join " "))
            $enrollProc = Start-Process -FilePath $agBin -ArgumentList $enrollArgList -PassThru -NoNewWindow `
                -RedirectStandardOutput $agEnrollLog -RedirectStandardError $agEnrollErr
            $enrollProc.WaitForExit(30000) | Out-Null
            if ($enrollProc.ExitCode -eq 0) {
                Write-Ok "Agent enrolled successfully"
            } else {
                Write-Warn ("Agent enrollment exited with code " + $enrollProc.ExitCode)
                # Show enrollment error log
                if (Test-Path $agEnrollErr) {
                    $errContent = Get-Content $agEnrollErr -Raw
                    if ($errContent) { Write-Warn ("Enrollment error: " + $errContent.Substring(0, [Math]::Min(500, $errContent.Length))) }
                }
            }

            # --- Start agent with enrolled config ---
            $agConfigFile = Join-Path $agDataDir "agent.yaml"
            if (-not (Test-Path $agConfigFile)) {
                Write-Err ("Agent config not created at " + $agConfigFile + " - enrollment may have failed")
                Write-Warn "Starting agent without enrollment (fallback mode)..."
                $env:GATEWAY_URL = "wss://localhost:" + $gwPort
                $env:DATA_DIR = $agDataDir
                $agLog = Join-Path $script:LogDir ("agent-" + $siteName + ".log")
                $agErr = Join-Path $script:LogDir ("agent-" + $siteName + ".err.log")
                $agProc = Start-Process -FilePath $agBin -PassThru -NoNewWindow `
                    -RedirectStandardOutput $agLog -RedirectStandardError $agErr
            } else {
                $agArgList = @("--config", "`"$agConfigFile`"")
                $agLog = Join-Path $script:LogDir ("agent-" + $siteName + ".log")
                $agErr = Join-Path $script:LogDir ("agent-" + $siteName + ".err.log")
                $agProc = Start-Process -FilePath $agBin -ArgumentList $agArgList -PassThru -NoNewWindow `
                    -RedirectStandardOutput $agLog -RedirectStandardError $agErr
            }
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
# ADD-HOSTING
# ---------------------------------------------------------------------------
function Do-AddHosting {
    $hostingName = $Arg1
    if (-not $hostingName) {
        Write-Err "Usage: appcontrol.ps1 add-hosting <name> [description]"
        return
    }

    $description = $Arg2

    $settings = Read-Settings
    if (-not $settings) {
        Write-Err "No config/settings.json found. Run install first."
        return
    }

    $port = $settings.backend_port
    if (-not $port) { $port = "3000" }

    Write-Info ("Adding hosting '" + $hostingName + "'")

    # Login
    $adminEmail = $settings.admin_email
    $adminPass = $settings.admin_password
    $token = Login-Backend -Port $port -AdminEmail $adminEmail -AdminPassword $adminPass
    Write-Ok "Authenticated"

    $baseUrl = "http://localhost:" + $port + "/api/v1"

    # Check if hosting already exists
    $existingHostings = Invoke-Api -Method "GET" -Uri ($baseUrl + "/hostings") -Token $token
    $hostingId = $null

    if ($existingHostings -and $existingHostings.hostings) {
        foreach ($h in $existingHostings.hostings) {
            if ($h.name -eq $hostingName) {
                $hostingId = $h.id
                Write-Info ("Hosting '" + $hostingName + "' already exists with ID " + $hostingId)
                break
            }
        }
    }

    # Create hosting if needed
    if (-not $hostingId) {
        $body = @{ name = $hostingName }
        if ($description) { $body.description = $description }

        $created = Invoke-Api -Method "POST" -Uri ($baseUrl + "/hostings") -Body $body -Token $token
        if ($created -and $created.id) {
            $hostingId = $created.id
            Write-Ok ("Created hosting '" + $hostingName + "' with ID " + $hostingId)
        } else {
            Write-Err "Failed to create hosting"
            return
        }
    }

    Write-Ok ("Hosting '" + $hostingName + "' ready (ID: " + $hostingId + ")")
    Write-Info "Use 'assign-site-hosting <site> <hosting>' to assign sites."
}

# ---------------------------------------------------------------------------
# ASSIGN-SITE-HOSTING
# ---------------------------------------------------------------------------
function Do-AssignSiteHosting {
    $siteRef = $Arg1
    $hostingRef = $Arg2
    if (-not $siteRef -or -not $hostingRef) {
        Write-Err "Usage: appcontrol.ps1 assign-site-hosting <site-name-or-code-or-id> <hosting-name-or-id>"
        return
    }

    $settings = Read-Settings
    if (-not $settings) {
        Write-Err "No config/settings.json found. Run install first."
        return
    }

    $port = $settings.backend_port
    if (-not $port) { $port = "3000" }

    # Login
    $adminEmail = $settings.admin_email
    $adminPass = $settings.admin_password
    $token = Login-Backend -Port $port -AdminEmail $adminEmail -AdminPassword $adminPass

    $baseUrl = "http://localhost:" + $port + "/api/v1"

    # Find the site by name, code, or id
    $sitesResult = Invoke-Api -Method "GET" -Uri ($baseUrl + "/sites") -Token $token
    $sites = @()
    if ($sitesResult -and $sitesResult.sites) { $sites = @($sitesResult.sites) }
    if ($sitesResult -and $sitesResult.data) { $sites = @($sitesResult.data) }
    if ($sitesResult -is [array]) { $sites = $sitesResult }

    $matchedSite = $null
    foreach ($s in $sites) {
        if ($s.name -eq $siteRef -or $s.code -eq $siteRef -or $s.id -eq $siteRef) {
            $matchedSite = $s
            break
        }
    }
    if (-not $matchedSite) {
        Write-Err ("Site not found: " + $siteRef)
        return
    }

    # Find the hosting by name or id
    $hostingsResult = Invoke-Api -Method "GET" -Uri ($baseUrl + "/hostings") -Token $token
    $hostings = @()
    if ($hostingsResult -and $hostingsResult.hostings) { $hostings = @($hostingsResult.hostings) }

    $matchedHosting = $null
    foreach ($h in $hostings) {
        if ($h.name -eq $hostingRef -or $h.id -eq $hostingRef) {
            $matchedHosting = $h
            break
        }
    }
    if (-not $matchedHosting) {
        Write-Err ("Hosting not found: " + $hostingRef)
        return
    }

    # Assign site to hosting
    try {
        Invoke-Api -Method "PUT" -Uri ($baseUrl + "/sites/" + $matchedSite.id) -Body @{
            hosting_id = $matchedHosting.id
        } -Token $token | Out-Null
        Write-Ok ("Assigned site '" + $matchedSite.name + "' to hosting '" + $matchedHosting.name + "'")
    } catch {
        Write-Err ("Failed to assign site: " + $_)
    }
}

# ---------------------------------------------------------------------------
# IMPORT-MAP
# ---------------------------------------------------------------------------
function Do-ImportMap {
    $source = $Arg1
    if (-not $source) {
        Write-Err "Usage: appcontrol.ps1 import-map <path-or-url> [site-name]"
        Write-Host ""
        Write-Host "  Import an application map from a local JSON file or a URL." -ForegroundColor DarkGray
        Write-Host "  If the JSON contains binding_profiles with site references," -ForegroundColor DarkGray
        Write-Host "  sites and agents are resolved automatically." -ForegroundColor DarkGray
        Write-Host "  Examples:" -ForegroundColor DarkGray
        Write-Host "    .\appcontrol.ps1 import-map C:\maps\myapp.json" -ForegroundColor DarkGray
        Write-Host "    .\appcontrol.ps1 import-map https://example.com/maps/myapp.json Production" -ForegroundColor DarkGray
        return
    }

    $settings = Read-Settings
    if (-not $settings) {
        Write-Err "No config/settings.json found. Run install first."
        return
    }

    $port = $settings.backend_port
    if (-not $port) { $port = "3000" }
    $adminEmail = $settings.admin_email
    if (-not $adminEmail) { $adminEmail = $Email }
    $adminPassword = $settings.admin_password
    if (-not $adminPassword) { $adminPassword = $Password }

    # Determine if source is URL or file path
    $jsonContent = $null
    if ($source -match "^https?://") {
        Write-Info ("Downloading map from: " + $source)
        try {
            $response = Invoke-WebRequest -Uri $source -UseBasicParsing -ErrorAction Stop
            $jsonContent = $response.Content
            Write-Ok "Map downloaded successfully"
        } catch {
            Write-Err ("Failed to download map: " + $_)
            return
        }
    } else {
        # Local file
        if (-not (Test-Path $source)) {
            Write-Err ("File not found: " + $source)
            return
        }
        Write-Info ("Reading map from: " + $source)
        $jsonContent = Get-Content -Path $source -Raw -Encoding UTF8
        Write-Ok "Map file loaded"
    }

    # Validate JSON
    $parsed = $null
    try {
        $parsed = $jsonContent | ConvertFrom-Json
        $appData = if ($parsed.application) { $parsed.application } else { $parsed }
        Write-Info ("Application: " + $appData.name)
    } catch {
        Write-Err ("Invalid JSON: " + $_)
        return
    }

    # Login
    $baseUri = "http://localhost:" + $port
    try {
        $token = Login-Backend -Port $port -AdminEmail $adminEmail -AdminPassword $adminPassword
    } catch {
        Write-Err "Failed to login. Is the backend running?"
        return
    }

    # Get available sites with their gateways
    $gatewayResult = Invoke-Api -Method "GET" -Uri ($baseUri + "/api/v1/gateways") -Token $token
    $availableSites = @()
    if ($gatewayResult -and $gatewayResult.sites) { $availableSites = @($gatewayResult.sites) }

    if ($availableSites.Count -eq 0) {
        Write-Err "No sites with connected gateways found. Run 'add-site' first."
        return
    }

    # Extract binding_profiles from JSON to auto-detect sites
    $appData = if ($parsed.application) { $parsed.application } else { $parsed }
    $bindingProfiles = @()
    if ($appData.binding_profiles) { $bindingProfiles = @($appData.binding_profiles) }

    # Match binding_profiles to available sites
    $primarySite = $null
    $drSites = @()

    if ($bindingProfiles.Count -gt 0) {
        Write-Info ("Found " + $bindingProfiles.Count + " binding profile(s) in JSON")
        foreach ($bp in $bindingProfiles) {
            $siteCode = if ($bp.site -and $bp.site.code) { $bp.site.code } else { $null }
            $siteName = if ($bp.site -and $bp.site.name) { $bp.site.name } else { $null }
            if (-not $siteCode -and -not $siteName) { continue }

            $match = $availableSites | Where-Object {
                ($siteCode -and $_.site_code -eq $siteCode) -or ($siteName -and $_.site_name -eq $siteName)
            }
            if ($match) {
                if ($match -is [array]) { $match = $match[0] }
                $profileType = if ($bp.profile_type) { $bp.profile_type } else { "primary" }
                if ($profileType -ne "dr" -and -not $primarySite) {
                    $primarySite = $match
                    Write-Info ("  Primary site: " + $match.site_name + " (" + $match.site_code + ")")
                } else {
                    $drSites += $match
                    Write-Info ("  DR site: " + $match.site_name + " (" + $match.site_code + ")")
                }
            } else {
                $label = if ($siteCode) { $siteCode } else { $siteName }
                Write-Warn ("  Binding profile references site '$label' -- not found locally, skipping")
            }
        }
    }

    # If no primary site from binding_profiles, fall back to manual selection or argument
    if (-not $primarySite) {
        if ($Arg2) {
            $match = $availableSites | Where-Object { $_.site_name -eq $Arg2 -or $_.site_code -eq $Arg2 -or $_.site_id -eq $Arg2 }
            if ($match) {
                if ($match -is [array]) { $match = $match[0] }
                $primarySite = $match
                Write-Info ("Using site: " + $match.site_name)
            } else {
                Write-Err ("Site not found: " + $Arg2)
                return
            }
        } elseif ($availableSites.Count -eq 1) {
            $primarySite = $availableSites[0]
            Write-Info ("Using site: " + $primarySite.site_name)
        } else {
            Write-Host ""
            Write-Host "No binding_profiles found in JSON. Select primary site:" -ForegroundColor Yellow
            for ($i = 0; $i -lt $availableSites.Count; $i++) {
                $s = $availableSites[$i]
                $gwCount = if ($s.gateways) { @($s.gateways).Count } else { 0 }
                Write-Host ("  [" + ($i + 1) + "] " + $s.site_name + " (" + $s.site_code + ") -- " + $gwCount + " gateway(s)") -ForegroundColor White
            }
            Write-Host ""
            $choice = Read-Host "Select site number"
            $idx = [int]$choice - 1
            if ($idx -ge 0 -and $idx -lt $availableSites.Count) {
                $primarySite = $availableSites[$idx]
                Write-Info ("Using site: " + $primarySite.site_name)
            } else {
                Write-Err "Invalid selection."
                return
            }
        }
    }

    # Collect gateway IDs for primary and DR sites
    $primaryGatewayIds = @()
    if ($primarySite.gateways) {
        $primaryGatewayIds = @($primarySite.gateways | ForEach-Object { $_.id })
    }
    $drGatewayIds = @()
    foreach ($dr in $drSites) {
        if ($dr.gateways) {
            $drGatewayIds += @($dr.gateways | ForEach-Object { $_.id })
        }
    }

    Write-Info ("Primary gateways: " + $primaryGatewayIds.Count + ", DR gateways: " + $drGatewayIds.Count)

    # Step 1: Preview -- resolve agents
    $previewBody = @{
        content     = $jsonContent
        format      = "json"
        gateway_ids = $primaryGatewayIds
    }
    if ($drGatewayIds.Count -gt 0) {
        $previewBody["dr_gateway_ids"] = $drGatewayIds
    }

    Write-Info "Resolving agents..."
    $preview = $null
    try {
        $preview = Invoke-Api -Method "POST" -Uri ($baseUri + "/api/v1/import/preview") -Body $previewBody -Token $token
    } catch {
        Write-Err ("Preview failed: " + $_)
        return
    }

    if (-not $preview -or -not $preview.components) {
        Write-Err "Preview returned no components."
        return
    }

    Write-Info ("Application: " + $preview.application_name + " (" + $preview.component_count + " components)")

    if ($preview.existing_application) {
        Write-Warn ("Application '" + $preview.existing_application.name + "' already exists -- will update")
    }

    # Build host lookup from primary binding_profiles in JSON
    $bpHostLookup = @{}
    foreach ($bp in $bindingProfiles) {
        $profileType = if ($bp.profile_type) { $bp.profile_type } else { "primary" }
        if ($profileType -eq "dr") { continue }
        if ($bp.mappings) {
            foreach ($m in @($bp.mappings)) {
                if ($m.component_name -and $m.host) {
                    $bpHostLookup[$m.component_name] = $m.host
                }
            }
        }
    }

    # Build primary profile mappings from preview resolution + binding_profile fallback
    $primaryMappings = @()
    $unresolvedCount = 0
    foreach ($comp in $preview.components) {
        if ($comp.resolution.status -eq "resolved") {
            $primaryMappings += @{
                component_name = $comp.name
                agent_id       = $comp.resolution.agent_id
                resolved_via   = $comp.resolution.resolved_via
            }
        } elseif ($comp.resolution.status -eq "multiple") {
            $first = $comp.resolution.candidates[0]
            $primaryMappings += @{
                component_name = $comp.name
                agent_id       = $first.agent_id
                resolved_via   = "wizard"
            }
            Write-Warn ("  " + $comp.name + ": multiple agents, using " + $first.hostname)
        } else {
            # Try to resolve from binding_profile host mapping
            $bpHost = $bpHostLookup[$comp.name]
            $matchedAgent = $null
            if ($bpHost -and $preview.available_agents) {
                $matchedAgent = @($preview.available_agents) | Where-Object {
                    $_.hostname -eq $bpHost -or
                    $_.hostname -like "$bpHost.*" -or
                    $bpHost -like "$($_.hostname).*"
                } | Select-Object -First 1
            }
            if ($matchedAgent) {
                $primaryMappings += @{
                    component_name = $comp.name
                    agent_id       = $matchedAgent.agent_id
                    resolved_via   = "wizard"
                }
                Write-Info ("  " + $comp.name + ": resolved via binding_profile host '" + $bpHost + "' -> " + $matchedAgent.hostname)
            } elseif ($preview.available_agents -and @($preview.available_agents).Count -eq 1) {
                $primaryMappings += @{
                    component_name = $comp.name
                    agent_id       = $preview.available_agents[0].agent_id
                    resolved_via   = "wizard"
                }
            } else {
                $unresolvedCount++
                $hostInfo = if ($bpHost) { $bpHost } else { $comp.host }
                Write-Warn ("  " + $comp.name + ": no agent resolved (host: " + $hostInfo + ")")
                if ($preview.available_agents -and @($preview.available_agents).Count -gt 0) {
                    $primaryMappings += @{
                        component_name = $comp.name
                        agent_id       = $preview.available_agents[0].agent_id
                        resolved_via   = "wizard"
                    }
                }
            }
        }
    }

    if ($primaryMappings.Count -eq 0) {
        Write-Err "No component-to-agent mappings could be resolved. Are agents connected?"
        return
    }

    if ($unresolvedCount -gt 0) {
        Write-Warn ("$unresolvedCount component(s) could not be resolved by hostname -- used fallback agent")
    }

    # Build DR host lookups from binding_profiles
    $drBpHostLookups = @{}
    foreach ($bp in $bindingProfiles) {
        if ($bp.profile_type -ne "dr") { continue }
        $siteCode = if ($bp.site -and $bp.site.code) { $bp.site.code } else { $null }
        if (-not $siteCode) { continue }
        $lookup = @{}
        if ($bp.mappings) {
            foreach ($m in @($bp.mappings)) {
                if ($m.component_name -and $m.host) {
                    $lookup[$m.component_name] = $m.host
                }
            }
        }
        $drBpHostLookups[$siteCode] = $lookup
    }

    # Build DR profiles (one per DR site)
    $drProfiles = @()
    foreach ($dr in $drSites) {
        $drSiteGwIds = @()
        if ($dr.gateways) { $drSiteGwIds = @($dr.gateways | ForEach-Object { $_.id }) }
        $drAgents = @()
        if ($preview.dr_available_agents) {
            $gwSet = @{}
            foreach ($gid in $drSiteGwIds) { $gwSet[$gid] = $true }
            $drAgents = @($preview.dr_available_agents | Where-Object { $gwSet[$_.gateway_id] })
            if ($drAgents.Count -eq 0) { $drAgents = @($preview.dr_available_agents) }
        }

        # Get host lookup for this DR site
        $drHostLookup = $drBpHostLookups[$dr.site_code]

        $drMappings = @()
        foreach ($comp in $preview.components) {
            $mapped = $false

            # 1. Try DR suggestion from preview
            if ($preview.dr_suggestions) {
                $suggestion = $preview.dr_suggestions | Where-Object { $_.component_name -eq $comp.name }
                if ($suggestion -is [array]) { $suggestion = $suggestion[0] }
                if ($suggestion -and $suggestion.dr_resolution -and $suggestion.dr_resolution.status -eq "resolved") {
                    $suggestedId = $suggestion.dr_resolution.agent_id
                    $inSite = $drAgents | Where-Object { $_.agent_id -eq $suggestedId }
                    if ($inSite) {
                        $drMappings += @{
                            component_name = $comp.name
                            agent_id       = $suggestedId
                            resolved_via   = "wizard"
                        }
                        $mapped = $true
                    }
                }
            }

            # 2. Try binding_profile host mapping
            if (-not $mapped -and $drHostLookup -and $drHostLookup[$comp.name]) {
                $drHost = $drHostLookup[$comp.name]
                $matchedAgent = @($drAgents) | Where-Object {
                    $_.hostname -eq $drHost -or
                    $_.hostname -like "$drHost.*" -or
                    $drHost -like "$($_.hostname).*"
                } | Select-Object -First 1
                if ($matchedAgent) {
                    $drMappings += @{
                        component_name = $comp.name
                        agent_id       = $matchedAgent.agent_id
                        resolved_via   = "wizard"
                    }
                    $mapped = $true
                }
            }

            # 3. Fallback: first DR agent for this site
            if (-not $mapped -and $drAgents.Count -gt 0) {
                $drMappings += @{
                    component_name = $comp.name
                    agent_id       = $drAgents[0].agent_id
                    resolved_via   = "wizard"
                }
            }
        }

        $drProfileName = if ($dr.site_code) { $dr.site_code.ToLower() } else { "dr" }
        $drProfiles += @{
            name         = $drProfileName
            description  = ("DR configuration for " + $dr.site_name)
            profile_type = "dr"
            gateway_ids  = $drSiteGwIds
            auto_failover = $false
            mappings     = $drMappings
        }
        Write-Info ("DR profile '" + $drProfileName + "': " + $drMappings.Count + " mappings")
    }

    # Step 2: Execute import
    $profileName = if ($primarySite.site_code) { $primarySite.site_code.ToLower() } else { "primary" }
    $executeBody = @{
        content    = $jsonContent
        format     = "json"
        site_id    = $primarySite.site_id
        profile    = @{
            name         = $profileName
            description  = ("Primary configuration for " + $primarySite.site_name)
            profile_type = "primary"
            gateway_ids  = $primaryGatewayIds
            mappings     = $primaryMappings
        }
        conflict_action = if ($preview.existing_application) { "update" } else { "fail" }
    }
    if ($drProfiles.Count -gt 0) {
        $executeBody["dr_profiles"] = $drProfiles
    }

    Write-Info "Importing application..."
    try {
        $result = Invoke-Api -Method "POST" -Uri ($baseUri + "/api/v1/import/execute") -Body $executeBody -Token $token
        if ($result -and $result.application_id) {
            Write-Ok ("Application imported: " + $result.application_name + " (ID: " + $result.application_id + ")")
            Write-Info ("Components: " + $result.components_created + ", Profiles: " + ($result.profiles_created -join ", "))
            Write-Info ("Active profile: " + $result.active_profile)
            if ($result.warnings -and $result.warnings.Count -gt 0) {
                foreach ($w in $result.warnings) { Write-Warn $w }
            }
        } else {
            Write-Ok "Import completed."
        }
    } catch {
        Write-Err ("Import failed: " + $_)
        return
    }

    Write-Host ""
    Write-Ok "Map import complete"
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
    Write-Host "  add-hosting <name> [desc]  Add a hosting (group of sites)"
    Write-Host "  assign-site-hosting <site> <hosting>  Assign a site to a hosting"
    Write-Host "  import-example [name] [site]  Import an example application map"
    Write-Host "  import-map <path|url> [site]  Import a map from file or URL"
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
    Write-Host "  .\appcontrol.ps1 import-example                              # list examples"
    Write-Host "  .\appcontrol.ps1 import-example metrics-demo-windows         # auto-select site"
    Write-Host "  .\appcontrol.ps1 import-example metrics-demo-windows Production  # specify site"
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
    Write-Host "  examples/   Example application maps"
    Write-Host ""
}

# ---------------------------------------------------------------------------
# IMPORT-EXAMPLE
# ---------------------------------------------------------------------------
function Do-ImportExample {
    $settings = Read-Settings
    if (-not $settings) {
        Write-Err "No config/settings.json found. Run install first."
        return
    }

    $port = $settings.backend_port
    if (-not $port) { $port = "3000" }
    $adminEmail = $settings.admin_email
    if (-not $adminEmail) { $adminEmail = $Email }
    $adminPassword = $settings.admin_password
    if (-not $adminPassword) { $adminPassword = $Password }

    $examplesDir = Join-Path $script:ScriptDir "examples"

    # Download examples if not present (check for actual JSON files, not just empty dir)
    $exampleJsonFiles = Get-ChildItem -Path $examplesDir -Filter "*.json" -ErrorAction SilentlyContinue
    if (-not $exampleJsonFiles -or $exampleJsonFiles.Count -eq 0) {
        Write-Info "Downloading examples..."
        Ensure-Dir $examplesDir
        $downloaded = $false
        $tarFile = Join-Path $script:BinDir "examples.tar.gz"
        try {
            Download-File -Url ($script:ReleasesBase + "/examples.tar.gz") -OutPath $tarFile
            # Extract tar.gz (tar is available on Windows 10+)
            if ($script:IsWin) {
                tar -xzf $tarFile -C $script:ScriptDir 2>$null
            } else {
                tar xzf $tarFile -C $script:ScriptDir
            }
            Remove-Item $tarFile -Force -ErrorAction SilentlyContinue
            $downloaded = $true
            Write-Ok "Examples downloaded to examples/"
        } catch {
            Write-Warn "Could not download examples from release."
        }

        # Fallback: try appcontrol-docs-scripts.zip (corporate releases)
        if (-not $downloaded) {
            $docsZip = Join-Path $script:BinDir "appcontrol-docs-scripts.zip"
            try {
                Download-File -Url ($script:ReleasesBase + "/appcontrol-docs-scripts.zip") -OutPath $docsZip
                Expand-Archive -Path $docsZip -DestinationPath $script:ScriptDir -Force
                Remove-Item $docsZip -Force -ErrorAction SilentlyContinue
                $downloaded = $true
                Write-Ok "Examples extracted from docs-scripts package."
            } catch {
                Write-Warn "Could not download docs-scripts package."
            }
        }

        if (-not $downloaded) {
            Write-Err "No examples available. Place example JSON files in examples/ manually."
            return
        }
    }

    # List available examples
    $suffix = ""
    if ($script:IsWin) { $suffix = "-windows" }

    $jsonFiles = Get-ChildItem -Path $examplesDir -Filter "*.json" -ErrorAction SilentlyContinue
    if (-not $jsonFiles -or $jsonFiles.Count -eq 0) {
        Write-Err "No example files found in examples/"
        return
    }

    if (-not $Arg1) {
        Write-Host ""
        Write-Host "Available examples:" -ForegroundColor Yellow
        Write-Host ""
        foreach ($f in $jsonFiles) {
            $name = $f.BaseName
            $tag = ""
            if ($name -like "*-windows") { $tag = " (Windows)" }
            elseif ($jsonFiles | Where-Object { $_.BaseName -eq "$name-windows" }) { $tag = " (Linux/macOS)" }
            Write-Host ("  " + $name) -ForegroundColor White -NoNewline
            Write-Host $tag -ForegroundColor DarkGray
        }
        Write-Host ""
        Write-Host "Usage: .\appcontrol.ps1 import-example <name>" -ForegroundColor Cyan
        Write-Host ""
        if ($suffix) {
            Write-Host "Tip: Windows versions have a '-windows' suffix." -ForegroundColor DarkGray
        }
        return
    }

    # Resolve the example file
    $exampleName = $Arg1
    $exampleFile = Join-Path $examplesDir ($exampleName + ".json")
    if (-not (Test-Path $exampleFile)) {
        # Try with -windows suffix
        $exampleFile = Join-Path $examplesDir ($exampleName + "-windows.json")
        if (-not (Test-Path $exampleFile)) {
            Write-Err ("Example not found: " + $exampleName)
            Write-Info "Run '.\appcontrol.ps1 import-example' to list available examples."
            return
        }
    }

    Write-Info ("Importing: " + (Split-Path -Leaf $exampleFile))

    # Login
    $baseUri = "http://localhost:" + $port
    try {
        $token = Login-Backend -Port $port -AdminEmail $adminEmail -AdminPassword $adminPassword
    } catch {
        Write-Err "Failed to login. Is the backend running?"
        return
    }

    # Get available sites
    $sitesResult = Invoke-Api -Method "GET" -Uri ($baseUri + "/api/v1/sites") -Token $token
    $sites = @()
    if ($sitesResult -and $sitesResult.sites) { $sites = @($sitesResult.sites) }

    if ($sites.Count -eq 0) {
        Write-Err "No sites configured. Run 'add-site' first."
        return
    }

    # Select site
    $siteId = $null
    if ($Arg2) {
        # Site specified as argument
        $match = $sites | Where-Object { $_.site_name -eq $Arg2 -or $_.site_code -eq $Arg2 -or $_.id -eq $Arg2 }
        if ($match) {
            $siteId = $match.id
            Write-Info ("Using site: " + $match.site_name)
        } else {
            Write-Err ("Site not found: " + $Arg2)
            return
        }
    } elseif ($sites.Count -eq 1) {
        $siteId = $sites[0].id
        Write-Info ("Using site: " + $sites[0].site_name)
    } else {
        Write-Host ""
        Write-Host "Available sites:" -ForegroundColor Yellow
        for ($i = 0; $i -lt $sites.Count; $i++) {
            $s = $sites[$i]
            $stype = if ($s.site_type) { " (" + $s.site_type + ")" } else { "" }
            Write-Host ("  [" + ($i + 1) + "] " + $s.site_name + $stype) -ForegroundColor White
        }
        Write-Host ""
        $choice = Read-Host "Select site number"
        $idx = [int]$choice - 1
        if ($idx -ge 0 -and $idx -lt $sites.Count) {
            $siteId = $sites[$idx].id
            Write-Info ("Using site: " + $sites[$idx].site_name)
        } else {
            Write-Err "Invalid selection."
            return
        }
    }

    # Get agents on this site to show which host will be used
    $agentsResult = Invoke-Api -Method "GET" -Uri ($baseUri + "/api/v1/agents") -Token $token
    if ($agentsResult -and $agentsResult.agents) {
        $agentCount = @($agentsResult.agents).Count
        Write-Info ("Agents available: " + $agentCount)
        foreach ($a in $agentsResult.agents) {
            Write-Host ("  - " + $a.hostname + " (" + $a.id.Substring(0, 8) + ")") -ForegroundColor DarkGray
        }
    }

    # Read example and wrap in import envelope
    $jsonContent = Get-Content -Path $exampleFile -Raw -Encoding UTF8
    $importBody = @{
        json    = $jsonContent
        site_id = $siteId
    }

    try {
        $result = Invoke-Api -Method "POST" -Uri ($baseUri + "/api/v1/import/json") -Body $importBody -Token $token
        if ($result -and $result.application_id) {
            Write-Ok ("Application imported: " + $result.application_name + " (ID: " + $result.application_id + ")")
            Write-Info ("Components: " + $result.components_created + ", Dependencies: " + $result.dependencies_created)
            if ($result.warnings -and $result.warnings.Count -gt 0) {
                foreach ($w in $result.warnings) { Write-Warn $w }
            }
        } else {
            Write-Ok "Import completed."
        }
    } catch {
        Write-Err ("Import failed: " + $_)
        return
    }

    # Auto-assign agent to components (like the UI import wizard does)
    $appId = $null
    if ($result -and $result.application_id) { $appId = $result.application_id }

    if ($appId -and $agentsResult -and $agentsResult.agents) {
        $agentsList = @($agentsResult.agents)
        $selectedAgent = $null

        if ($agentsList.Count -eq 1) {
            $selectedAgent = $agentsList[0]
            Write-Info ("Auto-assigning agent: " + $selectedAgent.hostname)
        } elseif ($agentsList.Count -gt 1) {
            Write-Host ""
            Write-Host "Select agent to assign to all components:" -ForegroundColor Yellow
            for ($i = 0; $i -lt $agentsList.Count; $i++) {
                $ag = $agentsList[$i]
                Write-Host ("  [" + ($i + 1) + "] " + $ag.hostname) -ForegroundColor White
            }
            Write-Host ""
            $agChoice = Read-Host "Agent number (or Enter to skip)"
            if ($agChoice) {
                $agIdx = [int]$agChoice - 1
                if ($agIdx -ge 0 -and $agIdx -lt $agentsList.Count) {
                    $selectedAgent = $agentsList[$agIdx]
                }
            }
        }

        if ($selectedAgent) {
            # Get components of the imported app
            $appDetail = Invoke-Api -Method "GET" -Uri ($baseUri + "/api/v1/apps/" + $appId) -Token $token
            if ($appDetail -and $appDetail.components) {
                foreach ($comp in $appDetail.components) {
                    try {
                        Invoke-Api -Method "PUT" -Uri ($baseUri + "/api/v1/components/" + $comp.id) -Body @{
                            agent_id = $selectedAgent.id
                            host     = $selectedAgent.hostname
                        } -Token $token | Out-Null
                    } catch {
                        Write-Warn ("Failed to assign agent to " + $comp.name + ": " + $_)
                    }
                }
                Write-Ok ("Assigned " + $selectedAgent.hostname + " to " + @($appDetail.components).Count + " components")
            }
        }
    }

    # Windows metrics demo: copy check.bat, run setup.ps1, and fix check_cmd paths
    if ((Split-Path -Leaf $exampleFile) -eq "metrics-demo-windows.json") {
        $checkBat = Join-Path $examplesDir "metrics-demo-check.bat"
        $setupPs1 = Join-Path $examplesDir "metrics-demo-setup.ps1"

        if (Test-Path $checkBat) {
            $agentDir = $script:BinDir
            Copy-Item $checkBat -Destination $agentDir -Force
            Write-Ok ("Copied metrics-demo-check.bat to " + $agentDir)

            # Patch check_cmd with absolute path so the agent can find the .bat
            $batAbsPath = Join-Path $agentDir "metrics-demo-check.bat"
            if ($appId) {
                $appDetail2 = Invoke-Api -Method "GET" -Uri ($baseUri + "/api/v1/apps/" + $appId) -Token $token
                if ($appDetail2 -and $appDetail2.components) {
                    foreach ($comp in $appDetail2.components) {
                        if ($comp.check_cmd -and $comp.check_cmd -like "*metrics-demo-check.bat*") {
                            $newCheckCmd = $comp.check_cmd -replace "metrics-demo-check.bat", $batAbsPath
                            try {
                                Invoke-Api -Method "PUT" -Uri ($baseUri + "/api/v1/components/" + $comp.id) -Body @{
                                    check_cmd = $newCheckCmd
                                } -Token $token | Out-Null
                            } catch {
                                Write-Warn ("Failed to update check_cmd for " + $comp.name)
                            }
                        }
                    }
                    Write-Ok "Updated check commands with absolute path to metrics-demo-check.bat"
                }
            }
        }

        if (Test-Path $setupPs1) {
            Write-Info "Running metrics demo setup (creating JSON metrics files)..."
            & powershell -ExecutionPolicy Bypass -File $setupPs1
            Write-Ok "Metrics demo setup complete."
        }

        Write-Host ""
        Write-Info "Metrics demo ready. Run Start All from the UI to see metrics."
    }
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
    "add-site"       { Do-AddSite }
    "add-hosting"    { Do-AddHosting }
    "assign-site-hosting" { Do-AssignSiteHosting }
    "import-example" { Do-ImportExample }
    "import-map"     { Do-ImportMap }
    "upgrade"        { Do-Upgrade }
    "logs"           { Do-Logs }
    "help"           { Do-Help }
    default    { Do-Help }
}
