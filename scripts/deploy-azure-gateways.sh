#!/bin/bash
set -e

# ============================================================================
# Azure Gateway Deployment Script
# Deploys primary and failover gateways to Azure Container Instances
# ============================================================================

RESOURCE_GROUP="RG-APPCONTROL-CONTAINERS"
LOCATION="francecentral"
IMAGE="ghcr.io/fredericcarre/appcontrol-azure-gateway:latest"
SUBNET_ID="/subscriptions/06f79b51-9743-40e3-a2a7-65815c0583f8/resourceGroups/RG-APPCONTROL-CONTAINERS/providers/Microsoft.Network/virtualNetworks/APPCONTROL-CONTAINERS-VNET/subnets/APPCONTROL-CONTAINERS-SUBNET"
BACKEND_LOCAL_PORT=443
CLOUDFLARED_PID_FILE="/tmp/cloudflared-gateway.pid"
TUNNEL_URL_FILE="/tmp/cloudflared-tunnel-url.txt"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# ============================================================================
# Step 0: Parse arguments
# ============================================================================
DESTROY_ONLY=false
SKIP_CLOUDFLARE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --destroy)
            DESTROY_ONLY=true
            shift
            ;;
        --skip-cloudflare)
            SKIP_CLOUDFLARE=true
            shift
            ;;
        *)
            echo "Usage: $0 [--destroy] [--skip-cloudflare]"
            exit 1
            ;;
    esac
done

# ============================================================================
# Step 1: Destroy existing gateways
# ============================================================================
destroy_gateways() {
    log_info "Checking for existing Azure gateways..."

    # Delete primary gateway
    if az container show --resource-group "$RESOURCE_GROUP" --name "appcontrol-gateway-primary" &>/dev/null; then
        log_info "Deleting primary gateway..."
        az container delete --resource-group "$RESOURCE_GROUP" --name "appcontrol-gateway-primary" --yes
        log_success "Primary gateway deleted"
    fi

    # Delete failover gateway
    if az container show --resource-group "$RESOURCE_GROUP" --name "appcontrol-gateway-failover" &>/dev/null; then
        log_info "Deleting failover gateway..."
        az container delete --resource-group "$RESOURCE_GROUP" --name "appcontrol-gateway-failover" --yes
        log_success "Failover gateway deleted"
    fi
}

# Run destroy
destroy_gateways

if [ "$DESTROY_ONLY" = true ]; then
    log_success "Gateways destroyed. Exiting."
    exit 0
fi

# ============================================================================
# Step 2: Start Cloudflare tunnel
# ============================================================================
start_cloudflare_tunnel() {
    log_info "Starting Cloudflare tunnel..."

    # Kill existing cloudflared if running
    if [ -f "$CLOUDFLARED_PID_FILE" ]; then
        OLD_PID=$(cat "$CLOUDFLARED_PID_FILE")
        if kill -0 "$OLD_PID" 2>/dev/null; then
            log_info "Stopping existing cloudflared (PID $OLD_PID)..."
            kill "$OLD_PID" 2>/dev/null || true
            sleep 2
        fi
        rm -f "$CLOUDFLARED_PID_FILE"
    fi

    # Also kill any other cloudflared processes
    pkill -f "cloudflared.*tunnel.*localhost:$BACKEND_LOCAL_PORT" 2>/dev/null || true
    sleep 1

    # Start cloudflared in background with --no-tls-verify for self-signed certs
    rm -f "$TUNNEL_URL_FILE"
    cloudflared tunnel --no-tls-verify --url "https://localhost:$BACKEND_LOCAL_PORT" > /tmp/cloudflared.log 2>&1 &
    CLOUDFLARED_PID=$!
    echo "$CLOUDFLARED_PID" > "$CLOUDFLARED_PID_FILE"

    log_info "Waiting for tunnel URL (PID $CLOUDFLARED_PID)..."

    # Wait for tunnel URL to appear in logs
    for i in {1..30}; do
        sleep 1
        TUNNEL_URL=$(grep -oE 'https://[a-z0-9-]+\.trycloudflare\.com' /tmp/cloudflared.log 2>/dev/null | head -1)
        if [ -n "$TUNNEL_URL" ]; then
            echo "$TUNNEL_URL" > "$TUNNEL_URL_FILE"
            log_success "Tunnel URL: $TUNNEL_URL"
            return 0
        fi
        echo -n "."
    done

    log_error "Failed to get tunnel URL after 30 seconds"
    cat /tmp/cloudflared.log
    exit 1
}

if [ "$SKIP_CLOUDFLARE" = true ] && [ -f "$TUNNEL_URL_FILE" ]; then
    TUNNEL_URL=$(cat "$TUNNEL_URL_FILE")
    log_info "Using existing tunnel URL: $TUNNEL_URL"
else
    start_cloudflare_tunnel
    TUNNEL_URL=$(cat "$TUNNEL_URL_FILE")
fi

# Construct WebSocket URL for gateway
BACKEND_WS_URL="${TUNNEL_URL/https:/wss:}/ws/gateway"
log_info "Backend WebSocket URL: $BACKEND_WS_URL"

# ============================================================================
# Step 3: Create enrollment tokens
# ============================================================================
create_enrollment_token() {
    local GATEWAY_NAME=$1

    log_info "Creating enrollment token for $GATEWAY_NAME..."

    # Get admin token
    ADMIN_TOKEN=$(curl -s -X POST "https://localhost:$BACKEND_LOCAL_PORT/api/v1/auth/login" \
        -H "Content-Type: application/json" \
        -d '{"email":"admin@localhost","password":"admin"}' \
        --insecure | jq -r '.token')

    if [ -z "$ADMIN_TOKEN" ] || [ "$ADMIN_TOKEN" = "null" ]; then
        log_error "Failed to get admin token"
        exit 1
    fi

    # Create enrollment token (scope=gateway for gateway enrollment)
    RESPONSE=$(curl -s -X POST "https://localhost:$BACKEND_LOCAL_PORT/api/v1/enrollment/tokens" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer $ADMIN_TOKEN" \
        -d "{\"name\": \"$GATEWAY_NAME\", \"scope\": \"gateway\", \"valid_hours\": 168}" \
        --insecure)

    TOKEN=$(echo "$RESPONSE" | jq -r '.token')

    if [ -z "$TOKEN" ] || [ "$TOKEN" = "null" ]; then
        log_error "Failed to create enrollment token: $RESPONSE"
        exit 1
    fi

    log_success "Enrollment token created for $GATEWAY_NAME"
    echo "$TOKEN"
}

PRIMARY_TOKEN=$(create_enrollment_token "azure-gateway-primary")
FAILOVER_TOKEN=$(create_enrollment_token "azure-gateway-failover")

# ============================================================================
# Step 4: Deploy Azure Container Instances
# ============================================================================
deploy_gateway() {
    local NAME=$1
    local TOKEN=$2
    local IS_PRIMARY=$3

    log_info "Deploying $NAME to Azure..."

    az container create \
        --resource-group "$RESOURCE_GROUP" \
        --name "$NAME" \
        --image "$IMAGE" \
        --os-type Linux \
        --cpu 0.5 \
        --memory 0.5 \
        --restart-policy Always \
        --subnet "$SUBNET_ID" \
        --environment-variables \
            BACKEND_URL="$BACKEND_WS_URL" \
            GATEWAY_ENROLLMENT_TOKEN="$TOKEN" \
            GATEWAY_ZONE="azure-france" \
            GATEWAY_ID="$NAME" \
            GATEWAY_IS_PRIMARY="$IS_PRIMARY" \
            AZURE_AUTH_ENABLED="false" \
            RUST_LOG="info,appcontrol_gateway=debug" \
        --no-wait

    log_success "$NAME deployment initiated"
}

deploy_gateway "appcontrol-gateway-primary" "$PRIMARY_TOKEN" "true"
deploy_gateway "appcontrol-gateway-failover" "$FAILOVER_TOKEN" "false"

# ============================================================================
# Step 5: Wait for deployments and verify
# ============================================================================
log_info "Waiting for deployments to complete..."
sleep 30

check_gateway_status() {
    local NAME=$1
    STATE=$(az container show --resource-group "$RESOURCE_GROUP" --name "$NAME" --query "instanceView.state" -o tsv 2>/dev/null || echo "Unknown")
    echo "$STATE"
}

# Wait and check status
for i in {1..12}; do
    PRIMARY_STATE=$(check_gateway_status "appcontrol-gateway-primary")
    FAILOVER_STATE=$(check_gateway_status "appcontrol-gateway-failover")

    log_info "Primary: $PRIMARY_STATE | Failover: $FAILOVER_STATE"

    if [ "$PRIMARY_STATE" = "Running" ] && [ "$FAILOVER_STATE" = "Running" ]; then
        log_success "Both gateways are running!"
        break
    fi

    sleep 10
done

# ============================================================================
# Step 6: Verify gateways in backend
# ============================================================================
log_info "Checking gateways in backend..."
sleep 5

ADMIN_TOKEN=$(curl -s -X POST "https://localhost:$BACKEND_LOCAL_PORT/api/v1/auth/login" \
    -H "Content-Type: application/json" \
    -d '{"email":"admin@localhost","password":"admin"}' \
    --insecure | jq -r '.token')

GATEWAYS=$(curl -s "https://localhost:$BACKEND_LOCAL_PORT/api/v1/gateways" \
    -H "Authorization: Bearer $ADMIN_TOKEN" \
    --insecure)

echo ""
log_info "Registered gateways:"
echo "$GATEWAYS" | jq -r '.zones[] | select(.zone == "azure-france") | .gateways[] | "  - \(.name): role=\(.role), status=\(.status), connected=\(.connected)"'

# ============================================================================
# Summary
# ============================================================================
echo ""
echo "============================================================================"
log_success "Deployment complete!"
echo "============================================================================"
echo ""
echo "Cloudflare Tunnel URL: $TUNNEL_URL"
echo "Backend WebSocket URL: $BACKEND_WS_URL"
echo ""
echo "Azure Gateways (zone: azure-france):"
echo "  - appcontrol-gateway-primary (is_primary=true)"
echo "  - appcontrol-gateway-failover (is_primary=false)"
echo ""
echo "To view logs:"
echo "  az container logs --resource-group $RESOURCE_GROUP --name appcontrol-gateway-primary"
echo "  az container logs --resource-group $RESOURCE_GROUP --name appcontrol-gateway-failover"
echo ""
echo "To stop cloudflared when done:"
echo "  kill \$(cat $CLOUDFLARED_PID_FILE)"
echo ""
