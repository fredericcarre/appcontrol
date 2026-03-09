#!/bin/bash
set -euo pipefail

#
# Deploy AppControl Azure Gateway
#
# Usage:
#   ./deploy-azure-gateway.sh --backend-url <url> [--resource-group <name>] [--location <region>]
#
# Example:
#   ./deploy-azure-gateway.sh --backend-url "https://abc123.ngrok.io" --resource-group appcontrol-test
#

# Default values
RESOURCE_GROUP="${RESOURCE_GROUP:-appcontrol-test}"
LOCATION="${LOCATION:-westeurope}"
CONTAINER_NAME="${CONTAINER_NAME:-appcontrol-gateway}"
DNS_LABEL="${DNS_LABEL:-appcontrol-gw-$(openssl rand -hex 4)}"
VERSION="${VERSION:-latest}"
BACKEND_URL=""
GATEWAY_PORT="${GATEWAY_PORT:-4443}"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --backend-url)
            BACKEND_URL="$2"
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
        --version|-v)
            VERSION="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 --backend-url <url> [options]"
            echo ""
            echo "Options:"
            echo "  --backend-url <url>      Backend URL (required, e.g., https://abc.ngrok.io)"
            echo "  --resource-group, -g     Azure resource group (default: appcontrol-test)"
            echo "  --location, -l           Azure region (default: westeurope)"
            echo "  --name, -n               Container name (default: appcontrol-gateway)"
            echo "  --dns-label              DNS label for public IP (default: random)"
            echo "  --version, -v            Image version (default: latest)"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

if [[ -z "$BACKEND_URL" ]]; then
    echo "Error: --backend-url is required"
    echo "Run with --help for usage"
    exit 1
fi

echo "=== AppControl Azure Gateway Deployment ==="
echo "Backend URL:    $BACKEND_URL"
echo "Resource Group: $RESOURCE_GROUP"
echo "Location:       $LOCATION"
echo "Container:      $CONTAINER_NAME"
echo "DNS Label:      $DNS_LABEL"
echo "Version:        $VERSION"
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

# Create resource group if needed
echo "Creating resource group (if not exists)..."
az group create --name "$RESOURCE_GROUP" --location "$LOCATION" --output none 2>/dev/null || true

# Delete existing container if exists
echo "Checking for existing container..."
if az container show --resource-group "$RESOURCE_GROUP" --name "$CONTAINER_NAME" &>/dev/null; then
    echo "Deleting existing container..."
    az container delete --resource-group "$RESOURCE_GROUP" --name "$CONTAINER_NAME" --yes
    sleep 5
fi

# Deploy container
echo "Deploying gateway container..."
az container create \
    --resource-group "$RESOURCE_GROUP" \
    --name "$CONTAINER_NAME" \
    --image "ghcr.io/fredericcarre/appcontrol-azure-gateway:$VERSION" \
    --ports "$GATEWAY_PORT" \
    --ip-address Public \
    --dns-name-label "$DNS_LABEL" \
    --environment-variables \
        BACKEND_URL="$BACKEND_URL" \
        RUST_LOG="info,appcontrol=debug" \
        GATEWAY_PORT="$GATEWAY_PORT" \
    --cpu 1 \
    --memory 1 \
    --restart-policy Always \
    --output table

# Get container info
echo ""
echo "=== Deployment Complete ==="
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
echo ""
echo "To check logs:"
echo "  az container logs --resource-group $RESOURCE_GROUP --name $CONTAINER_NAME --follow"
echo ""
echo "To delete:"
echo "  az container delete --resource-group $RESOURCE_GROUP --name $CONTAINER_NAME --yes"
echo ""
echo "=== Next Steps ==="
echo "1. Create an enrollment token on your backend"
echo "2. On your Windows VM, run:"
echo ""
echo "   .\\install-agent-windows.ps1 \\"
echo "       -GatewayUrl \"wss://$FQDN:$GATEWAY_PORT\" \\"
echo "       -EnrollmentToken \"<your-token>\""
