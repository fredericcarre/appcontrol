#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Install AppControl Agent on Windows

.DESCRIPTION
    Downloads and installs the AppControl agent, enrolls with a gateway,
    and optionally installs as a Windows service.

.PARAMETER GatewayUrl
    The WebSocket URL of the gateway (e.g., wss://gateway.example.com:4443)

.PARAMETER EnrollmentToken
    The enrollment token provided by the administrator

.PARAMETER InstallDir
    Installation directory (default: C:\Program Files\AppControl)

.PARAMETER AsService
    Install as a Windows service

.PARAMETER Version
    Specific version to install (default: latest)

.EXAMPLE
    .\install-agent-windows.ps1 -GatewayUrl "wss://mygateway.azurecontainer.io:4443" -EnrollmentToken "abc123"

.EXAMPLE
    .\install-agent-windows.ps1 -GatewayUrl "wss://mygateway:4443" -EnrollmentToken "abc123" -AsService
#>

param(
    [Parameter(Mandatory=$true)]
    [string]$GatewayUrl,

    [Parameter(Mandatory=$true)]
    [string]$EnrollmentToken,

    [string]$InstallDir = "C:\Program Files\AppControl",

    [switch]$AsService,

    [string]$Version = "latest"
)

$ErrorActionPreference = "Stop"

Write-Host "=== AppControl Agent Installer ===" -ForegroundColor Cyan

# Determine version
if ($Version -eq "latest") {
    Write-Host "Fetching latest release..."
    $release = Invoke-RestMethod "https://api.github.com/repos/fredericcarre/appcontrol/releases/latest"
    $Version = $release.tag_name
}

Write-Host "Installing version: $Version" -ForegroundColor Green

# Create install directory
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$DataDir = Join-Path $InstallDir "data"
if (-not (Test-Path $DataDir)) {
    New-Item -ItemType Directory -Path $DataDir -Force | Out-Null
}

# Download agent binary
$arch = if ([Environment]::Is64BitOperatingSystem) { "amd64" } else { "x86" }
$binaryName = "appcontrol-agent-windows-$arch.exe"
$downloadUrl = "https://github.com/fredericcarre/appcontrol/releases/download/$Version/$binaryName"
$agentPath = Join-Path $InstallDir "appcontrol-agent.exe"

Write-Host "Downloading $binaryName..."
Invoke-WebRequest -Uri $downloadUrl -OutFile $agentPath -UseBasicParsing

# Create configuration
$configPath = Join-Path $InstallDir "agent.yaml"
$config = @"
agent:
  id: auto
  data_dir: "$($DataDir -replace '\\', '\\\\')"

gateway:
  url: "$GatewayUrl"
  reconnect_interval_secs: 10

tls:
  enabled: true
  skip_verify: false

labels:
  os: windows
  hostname: $env:COMPUTERNAME
"@

$config | Out-File -FilePath $configPath -Encoding UTF8
Write-Host "Configuration written to $configPath" -ForegroundColor Green

# Enroll the agent
Write-Host "Enrolling agent with gateway..."
$enrollResult = & $agentPath enroll --config $configPath --token $EnrollmentToken 2>&1

if ($LASTEXITCODE -ne 0) {
    Write-Host "Enrollment failed: $enrollResult" -ForegroundColor Red
    exit 1
}

Write-Host "Agent enrolled successfully!" -ForegroundColor Green

# Install as service if requested
if ($AsService) {
    Write-Host "Installing as Windows service..."

    # Check if service already exists
    $existingService = Get-Service -Name "AppControlAgent" -ErrorAction SilentlyContinue
    if ($existingService) {
        Write-Host "Stopping existing service..."
        Stop-Service -Name "AppControlAgent" -Force
        sc.exe delete "AppControlAgent" | Out-Null
        Start-Sleep -Seconds 2
    }

    # Create the service using sc.exe
    $binPath = "`"$agentPath`" run --config `"$configPath`""
    sc.exe create "AppControlAgent" binPath= $binPath start= auto displayname= "AppControl Agent"
    sc.exe description "AppControlAgent" "AppControl monitoring and control agent"

    # Start the service
    Start-Service -Name "AppControlAgent"

    Write-Host "Service 'AppControlAgent' installed and started!" -ForegroundColor Green
    Get-Service -Name "AppControlAgent" | Format-Table -AutoSize
} else {
    Write-Host ""
    Write-Host "To run the agent manually:" -ForegroundColor Yellow
    Write-Host "  & `"$agentPath`" run --config `"$configPath`""
    Write-Host ""
    Write-Host "To install as a service later:" -ForegroundColor Yellow
    Write-Host "  .\install-agent-windows.ps1 -GatewayUrl `"$GatewayUrl`" -EnrollmentToken `"<new-token>`" -AsService"
}

# Test discovery (optional)
Write-Host ""
Write-Host "Testing discovery scan..." -ForegroundColor Cyan
$discoveryResult = & $agentPath discovery --config $configPath 2>&1

if ($LASTEXITCODE -eq 0) {
    Write-Host "Discovery completed. Sample output:" -ForegroundColor Green
    $discoveryResult | Select-Object -First 50
} else {
    Write-Host "Discovery test skipped (agent may need to connect first)" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "=== Installation Complete ===" -ForegroundColor Cyan
Write-Host "Agent binary: $agentPath"
Write-Host "Configuration: $configPath"
Write-Host "Data directory: $DataDir"
