#!/bin/bash
set -euo pipefail

#
# Deploy AppControl Azure Gateway
#
# Usage:
#   ./deploy-azure-gateway.sh --backend-url <url> --gateway-token <token> [options]
#
# Example (public):
#   ./deploy-azure-gateway.sh \
#       --backend-url "https://appcontrol.example.com" \
#       --gateway-token "ac_enroll_xxx" \
#       --resource-group appcontrol-test
#
# Example (private VNet):
#   ./deploy-azure-gateway.sh \
#       --backend-url "https://appcontrol.example.com" \
#       --gateway-token "ac_enroll_xxx" \
#       --resource-group RG-APPCONTROL-CONTAINERS \
#       --subnet "/subscriptions/.../subnets/APPCONTROL-CONTAINERS-SUBNET" \
#       --private
#

# Default values
RESOURCE_GROUP="${RESOURCE_GROUP:-appcontrol-test}"
LOCATION="${LOCATION:-westeurope}"
CONTAINER_NAME="${CONTAINER_NAME:-appcontrol-gateway}"
DNS_LABEL=""
VERSION="${VERSION:-latest}"
BACKEND_URL=""
GATEWAY_PORT="${GATEWAY_PORT:-4443}"
GATEWAY_ID="${GATEWAY_ID:-}"
GATEWAY_ZONE="${GATEWAY_ZONE:-azure}"
GATEWAY_TOKEN=""
AGENT_TOKEN=""
SUBNET=""
PRIVATE_IP=false
AZURE_AUTH="${AZURE_AUTH:-false}"
CPU="${CPU:-2}"
MEMORY="${MEMORY:-2}"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --backend-url)
            BACKEND_URL="$2"
            shift 2
            ;;
        --gateway-token)
            GATEWAY_TOKEN="$2"
            shift 2
            ;;
        --agent-token)
            AGENT_TOKEN="$2"
            shift 2
            ;;
        --gateway-id)
            GATEWAY_ID="$2"
            shift 2
            ;;
        --gateway-zone)
            GATEWAY_ZONE="$2"
            shift 2
            ;;
        --resource-group|-g)
            RESOURCE_GROUP="$2"
            shift 2
            ;;
        --location|-l)
            LOCATION="$2"
            shift 2
            ;;
        --name|-n)
            CONTAINER_NAME="$2"
            shift 2
            ;;
        --dns-label)
            DNS_LABEL="$2"
            shift 2
            ;;
        --subnet)
            SUBNET="$2"
            shift 2
            ;;
        --private)
            PRIVATE_IP=true
            shift
            ;;
        --azure-auth)
            AZURE_AUTH=true
            shift
            ;;
        --version|-v)
            VERSION="$2"
            shift 2
            ;;
        --cpu)
            CPU="$2"
            shift 2
            ;;
        --memory)
            MEMORY="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 --backend-url <url> --gateway-token <token> [options]"
            echo ""
            echo "Required:"
            echo "  --backend-url <url>      Backend WebSocket URL (e.g., wss://appcontrol.example.com)"
            echo "  --gateway-token <token>  Gateway enrollment token (ac_enroll_xxx)"
            echo ""
            echo "Options:"
            echo "  --agent-token <token>    Agent enrollment token (for embedded agent)"
            echo "  --gateway-id <id>        Gateway identifier (default: container name)"
            echo "  --gateway-zone <zone>    Gateway zone (default: azure)"
            echo "  --resource-group, -g     Azure resource group (default: appcontrol-test)"
            echo "  --location, -l           Azure region (default: westeurope)"
            echo "  --name, -n               Container name (default: appcontrol-gateway)"
            echo "  --dns-label              DNS label for public IP"
            echo "  --subnet <id>            Subnet ID for private deployment"
            echo "  --private                Use private IP (requires --subnet)"
            echo "  --azure-auth             Enable Azure Managed Identity login"
            echo "  --version, -v            Image version (default: latest)"
            echo "  --cpu <n>                CPU cores (default: 2)"
            echo "  --memory <n>             Memory in GB (default: 2)"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Validation
if [[ -z "$BACKEND_URL" ]]; then
    echo "Error: --backend-url is required"
    echo "Run with --help for usage"
    exit 1
fi

if [[ -z "$GATEWAY_TOKEN" ]]; then
    echo "Error: --gateway-token is required"
    echo "Run with --help for usage"
    exit 1
fi

if [[ "$PRIVATE_IP" = true && -z "$SUBNET" ]]; then
    echo "Error: --subnet is required when using --private"
    exit 1
fi

# Set defaults
GATEWAY_ID="${GATEWAY_ID:-$CONTAINER_NAME}"
if [[ -z "$DNS_LABEL" && "$PRIVATE_IP" = false ]]; then
    DNS_LABEL="appcontrol-gw-$(openssl rand -hex 4)"
fi

echo "=== AppControl Azure Gateway Deployment ==="
echo "Backend URL:    $BACKEND_URL"
echo "Gateway ID:     $GATEWAY_ID"
echo "Gateway Zone:   $GATEWAY_ZONE"
echo "Resource Group: $RESOURCE_GROUP"
echo "Location:       $LOCATION"
echo "Container:      $CONTAINER_NAME"
echo "IP Type:        $([ "$PRIVATE_IP" = true ] && echo "Private" || echo "Public")"
[[ -n "$DNS_LABEL" ]] && echo "DNS Label:      $DNS_LABEL"
[[ -n "$SUBNET" ]] && echo "Subnet:         $SUBNET"
echo "Version:        $VERSION"
echo "CPU/Memory:     $CPU cores / $MEMORY GB"
echo ""

# Check Azure CLI
if ! command -v az &> /dev/null; then
    echo "Error: Azure CLI (az) not found. Install from https://aka.ms/installazurecli"
    exit 1
fi

# Check login
if ! az account show &> /dev/null; then
    echo "Not logged in to Azure. Running 'az login'..."
    az login
fi

# Create resource group if needed (only for new deployments)
if [[ "$PRIVATE_IP" = false ]]; then
    echo "Creating resource group (if not exists)..."
    az group create --name "$RESOURCE_GROUP" --location "$LOCATION" --output none 2>/dev/null || true
fi

# Delete existing container if exists
echo "Checking for existing container..."
if az container show --resource-group "$RESOURCE_GROUP" --name "$CONTAINER_NAME" &>/dev/null; then
    echo "Deleting existing container..."
    az container delete --resource-group "$RESOURCE_GROUP" --name "$CONTAINER_NAME" --yes
    sleep 5
fi

# Build environment variables
ENV_VARS="AZURE_AUTH_ENABLED=$AZURE_AUTH"
ENV_VARS="$ENV_VARS BACKEND_URL=$BACKEND_URL"
ENV_VARS="$ENV_VARS GATEWAY_ID=$GATEWAY_ID"
ENV_VARS="$ENV_VARS GATEWAY_ZONE=$GATEWAY_ZONE"
ENV_VARS="$ENV_VARS GATEWAY_ENROLLMENT_TOKEN=$GATEWAY_TOKEN"
ENV_VARS="$ENV_VARS RUST_LOG=info,appcontrol=debug"

if [[ -n "$AGENT_TOKEN" ]]; then
    ENV_VARS="$ENV_VARS AGENT_ENROLLMENT_TOKEN=$AGENT_TOKEN"
fi

# Build az container create command
echo "Deploying gateway container..."
AZ_CMD="az container create \
    --resource-group $RESOURCE_GROUP \
    --name $CONTAINER_NAME \
    --image ghcr.io/fredericcarre/appcontrol-azure-gateway:$VERSION \
    --os-type Linux \
    --ports $GATEWAY_PORT \
    --cpu $CPU \
    --memory $MEMORY \
    --restart-policy Always \
    --environment-variables $ENV_VARS"

if [[ "$PRIVATE_IP" = true ]]; then
    AZ_CMD="$AZ_CMD --ip-address Private --subnet $SUBNET"
else
    AZ_CMD="$AZ_CMD --ip-address Public --dns-name-label $DNS_LABEL"
fi

# Execute
eval "$AZ_CMD --output table"

# Get container info
echo ""
echo "=== Deployment Complete ==="

if [[ "$PRIVATE_IP" = true ]]; then
    IP=$(az container show \
        --resource-group "$RESOURCE_GROUP" \
        --name "$CONTAINER_NAME" \
        --query "ipAddress.ip" -o tsv)
    echo ""
    echo "Gateway IP (Private): $IP"
    echo "Gateway URL:          wss://$IP:$GATEWAY_PORT"
else
    FQDN=$(az container show \
        --resource-group "$RESOURCE_GROUP" \
        --name "$CONTAINER_NAME" \
        --query "ipAddress.fqdn" -o tsv)
    IP=$(az container show \
        --resource-group "$RESOURCE_GROUP" \
        --name "$CONTAINER_NAME" \
        --query "ipAddress.ip" -o tsv)
    echo ""
    echo "Gateway FQDN: $FQDN"
    echo "Gateway IP:   $IP"
    echo "Gateway URL:  wss://$FQDN:$GATEWAY_PORT"
fi

echo ""
echo "To check logs:"
echo "  az container logs --resource-group $RESOURCE_GROUP --name $CONTAINER_NAME --follow"
echo ""
echo "To delete:"
echo "  az container delete --resource-group $RESOURCE_GROUP --name $CONTAINER_NAME --yes"
echo ""
echo "=== Next Steps ==="
echo "1. Create an agent enrollment token on your backend"
echo "2. On your Windows VM, download and enroll the agent:"
echo ""
echo "   # Download from GitHub releases"
echo "   Invoke-WebRequest -Uri 'https://github.com/fredericcarre/appcontrol/releases/download/v$VERSION/appcontrol-agent-windows-amd64.zip' -OutFile agent.zip"
echo "   Expand-Archive agent.zip -DestinationPath ."
echo ""
echo "   # Enroll"
if [[ "$PRIVATE_IP" = true ]]; then
    echo "   .\\appcontrol-agent.exe enroll --gateway-url 'wss://$IP:$GATEWAY_PORT' --token '<agent-token>'"
else
    echo "   .\\appcontrol-agent.exe enroll --gateway-url 'wss://$FQDN:$GATEWAY_PORT' --token '<agent-token>'"
fi
