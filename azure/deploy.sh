#!/usr/bin/env bash
# =============================================================================
# AppControl — Azure Container Instance Deployment
# =============================================================================
# Deploys the gateway+agent container on Azure Container Instances (ACI)
# with Managed Identity for VM start/stop operations.
#
# Usage:
#   ./deploy.sh                    # Deploy with defaults
#   ./deploy.sh --build            # Build and push image, then deploy
#   ./deploy.sh --delete           # Remove all Azure resources
#   ./deploy.sh --status           # Show container status and logs
#
# Prerequisites:
#   1. az CLI installed and logged in (az login)
#   2. Docker (only if --build)
#   3. Backend deployed on OpenShift (need the WebSocket URL)
# =============================================================================

set -euo pipefail

# ── Configurable variables ────────────────────────────────────────────────────
RESOURCE_GROUP="${RESOURCE_GROUP:-appcontrol-rg}"
LOCATION="${LOCATION:-westeurope}"
CONTAINER_NAME="${CONTAINER_NAME:-appcontrol-gateway}"
IDENTITY_NAME="${IDENTITY_NAME:-appcontrol-identity}"

# Image — use GHCR or your own ACR
IMAGE="${IMAGE:-ghcr.io/fredericcarre/appcontrol-azure-gateway:latest}"

# Backend URL (OpenShift WebSocket endpoint)
BACKEND_URL="${BACKEND_URL:?ERROR: BACKEND_URL is required. Example: wss://appcontrol-api-user-dev.apps.sandbox.openshiftapps.com/ws/gateway}"

# Gateway configuration
GATEWAY_ID="${GATEWAY_ID:-azure-gateway-01}"
GATEWAY_ZONE="${GATEWAY_ZONE:-azure}"

# Container resources
CPU="${CPU:-1}"
MEMORY="${MEMORY:-1.5}"

# Target VMs resource group (for RBAC role assignment)
VM_RESOURCE_GROUP="${VM_RESOURCE_GROUP:-${RESOURCE_GROUP}}"

# ACR (only used with --build)
ACR_NAME="${ACR_NAME:-}"

# ── Colors ────────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info()  { echo -e "${BLUE}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

# ── Preflight ─────────────────────────────────────────────────────────────────
check_prerequisites() {
    if ! command -v az &>/dev/null; then
        error "Azure CLI not found. Install: https://docs.microsoft.com/cli/azure/install-azure-cli"
        exit 1
    fi

    if ! az account show &>/dev/null 2>&1; then
        error "Not logged in to Azure. Run: az login"
        exit 1
    fi

    info "Logged in as: $(az account show --query user.name -o tsv)"
    info "Subscription: $(az account show --query name -o tsv)"
}

# ── Build and push image ─────────────────────────────────────────────────────
build_image() {
    if [ -z "${ACR_NAME}" ]; then
        error "ACR_NAME is required for --build. Example: ACR_NAME=myacr ./deploy.sh --build"
        exit 1
    fi

    info "Building Azure gateway image..."
    local repo_root
    repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

    docker build -t "${ACR_NAME}.azurecr.io/appcontrol-azure-gateway:latest" \
        -f "${repo_root}/docker/Dockerfile.azure-gateway" \
        "${repo_root}"

    info "Pushing to ACR: ${ACR_NAME}..."
    az acr login --name "${ACR_NAME}"
    docker push "${ACR_NAME}.azurecr.io/appcontrol-azure-gateway:latest"

    IMAGE="${ACR_NAME}.azurecr.io/appcontrol-azure-gateway:latest"
    ok "Image pushed: ${IMAGE}"
}

# ── Create managed identity ──────────────────────────────────────────────────
create_identity() {
    info "Creating resource group: ${RESOURCE_GROUP}..."
    az group create --name "${RESOURCE_GROUP}" --location "${LOCATION}" --output none

    info "Creating managed identity: ${IDENTITY_NAME}..."
    az identity create \
        --resource-group "${RESOURCE_GROUP}" \
        --name "${IDENTITY_NAME}" \
        --output none

    # Get identity details
    local identity_id principal_id
    identity_id=$(az identity show \
        --resource-group "${RESOURCE_GROUP}" \
        --name "${IDENTITY_NAME}" \
        --query id -o tsv)
    principal_id=$(az identity show \
        --resource-group "${RESOURCE_GROUP}" \
        --name "${IDENTITY_NAME}" \
        --query principalId -o tsv)

    ok "Managed identity created: ${IDENTITY_NAME}"
    info "Principal ID: ${principal_id}"

    # Assign VM Contributor role on target resource group
    info "Assigning 'Virtual Machine Contributor' role on resource group: ${VM_RESOURCE_GROUP}..."
    local rg_id
    rg_id=$(az group show --name "${VM_RESOURCE_GROUP}" --query id -o tsv 2>/dev/null || echo "")

    if [ -z "${rg_id}" ]; then
        warn "Resource group '${VM_RESOURCE_GROUP}' not found. Create it first or set VM_RESOURCE_GROUP."
        warn "You can assign the role later with:"
        echo "  az role assignment create --assignee ${principal_id} --role 'Virtual Machine Contributor' --scope /subscriptions/<sub>/resourceGroups/${VM_RESOURCE_GROUP}"
    else
        az role assignment create \
            --assignee "${principal_id}" \
            --role "Virtual Machine Contributor" \
            --scope "${rg_id}" \
            --output none 2>/dev/null || warn "Role assignment may already exist"
        ok "Role assigned: VM Contributor on ${VM_RESOURCE_GROUP}"
    fi

    echo "${identity_id}"
}

# ── Deploy container ─────────────────────────────────────────────────────────
deploy() {
    check_prerequisites

    info "============================================"
    info "  AppControl Azure Gateway Deployment"
    info "============================================"
    info "Resource Group:  ${RESOURCE_GROUP}"
    info "Location:        ${LOCATION}"
    info "Container:       ${CONTAINER_NAME}"
    info "Image:           ${IMAGE}"
    info "Backend URL:     ${BACKEND_URL}"
    info "Gateway ID:      ${GATEWAY_ID}"
    info "Gateway Zone:    ${GATEWAY_ZONE}"
    info "============================================"
    echo ""

    # Step 1: Create identity and get role assignments
    local identity_id
    identity_id=$(create_identity)

    # Get identity client ID for login
    local client_id
    client_id=$(az identity show \
        --resource-group "${RESOURCE_GROUP}" \
        --name "${IDENTITY_NAME}" \
        --query clientId -o tsv)

    # Step 2: Deploy container
    info "Deploying container instance: ${CONTAINER_NAME}..."
    az container create \
        --resource-group "${RESOURCE_GROUP}" \
        --name "${CONTAINER_NAME}" \
        --image "${IMAGE}" \
        --assign-identity "${identity_id}" \
        --cpu "${CPU}" \
        --memory "${MEMORY}" \
        --os-type Linux \
        --restart-policy Always \
        --environment-variables \
            BACKEND_URL="${BACKEND_URL}" \
            GATEWAY_ID="${GATEWAY_ID}" \
            GATEWAY_ZONE="${GATEWAY_ZONE}" \
            AZURE_CLIENT_ID="${client_id}" \
            AZURE_AUTH_ENABLED="true" \
            RUST_LOG="info,appcontrol_gateway=debug,appcontrol_agent=debug" \
        --output none

    ok "Container deployed!"

    # Step 3: Wait for running state
    info "Waiting for container to start..."
    local state
    for i in $(seq 1 30); do
        state=$(az container show \
            --resource-group "${RESOURCE_GROUP}" \
            --name "${CONTAINER_NAME}" \
            --query instanceView.state -o tsv 2>/dev/null || echo "Unknown")
        if [ "${state}" = "Running" ]; then
            break
        fi
        sleep 5
    done

    if [ "${state}" = "Running" ]; then
        ok "Container is running!"
    else
        warn "Container state: ${state}. Check logs with: ./deploy.sh --status"
    fi

    # Step 4: Summary
    echo ""
    info "============================================"
    ok "  Deployment complete!"
    info "============================================"
    echo ""
    echo -e "  Container:    ${GREEN}${CONTAINER_NAME}${NC}"
    echo -e "  State:        ${GREEN}${state}${NC}"
    echo -e "  Identity:     ${BLUE}${IDENTITY_NAME}${NC} (${client_id})"
    echo -e "  Backend:      ${BLUE}${BACKEND_URL}${NC}"
    echo ""
    echo -e "  ${YELLOW}View logs:${NC}     ./deploy.sh --status"
    echo -e "  ${YELLOW}Azure portal:${NC}  https://portal.azure.com/#@/resource/subscriptions/$(az account show --query id -o tsv)/resourceGroups/${RESOURCE_GROUP}"
    echo ""
    echo -e "  ${YELLOW}To start/stop VMs, create components in AppControl with commands like:${NC}"
    echo '    check_cmd:  az vm show -g my-rg -n my-vm --query "powerState" -o tsv | grep -q running'
    echo '    start_cmd:  az vm start -g my-rg -n my-vm --no-wait'
    echo '    stop_cmd:   az vm deallocate -g my-rg -n my-vm --no-wait'
    echo ""
}

# ── Status ────────────────────────────────────────────────────────────────────
show_status() {
    check_prerequisites
    info "Container status:"
    az container show \
        --resource-group "${RESOURCE_GROUP}" \
        --name "${CONTAINER_NAME}" \
        --query '{state:instanceView.state, restartCount:containers[0].instanceView.restartCount, image:containers[0].image}' \
        --output table

    echo ""
    info "Recent logs:"
    az container logs \
        --resource-group "${RESOURCE_GROUP}" \
        --name "${CONTAINER_NAME}" \
        --tail 50
}

# ── Delete ────────────────────────────────────────────────────────────────────
delete_all() {
    check_prerequisites
    warn "This will delete:"
    warn "  - Container instance: ${CONTAINER_NAME}"
    warn "  - Managed identity: ${IDENTITY_NAME}"
    warn "  - Resource group: ${RESOURCE_GROUP} (and ALL resources in it)"
    echo ""
    read -r -p "Are you sure? (y/N): " confirm
    if [ "${confirm}" != "y" ] && [ "${confirm}" != "Y" ]; then
        info "Cancelled."
        exit 0
    fi

    info "Deleting resource group: ${RESOURCE_GROUP}..."
    az group delete --name "${RESOURCE_GROUP}" --yes --no-wait
    ok "Deletion initiated (runs in background)."
}

# ── Entry point ───────────────────────────────────────────────────────────────
case "${1:-}" in
    --build)
        check_prerequisites
        build_image
        deploy
        ;;
    --delete|--teardown|--remove)
        delete_all
        ;;
    --status|--logs)
        show_status
        ;;
    --help|-h)
        echo "Usage: $0 [--build|--delete|--status]"
        echo ""
        echo "Commands:"
        echo "  (default)    Deploy container to ACI"
        echo "  --build      Build image, push to ACR, then deploy"
        echo "  --delete     Delete all Azure resources"
        echo "  --status     Show container status and logs"
        echo ""
        echo "Required environment variables:"
        echo "  BACKEND_URL          Backend WebSocket URL (wss://...)"
        echo ""
        echo "Optional environment variables:"
        echo "  RESOURCE_GROUP       Azure resource group (default: appcontrol-rg)"
        echo "  LOCATION             Azure region (default: westeurope)"
        echo "  CONTAINER_NAME       ACI container name (default: appcontrol-gateway)"
        echo "  IDENTITY_NAME        Managed identity name (default: appcontrol-identity)"
        echo "  GATEWAY_ID           Gateway identifier (default: azure-gateway-01)"
        echo "  GATEWAY_ZONE         Gateway zone (default: azure)"
        echo "  VM_RESOURCE_GROUP    Target RG for VM role assignment"
        echo "  IMAGE                Docker image (default: ghcr.io/fredericcarre/appcontrol-azure-gateway:latest)"
        echo "  ACR_NAME             ACR name (required for --build)"
        echo "  CPU                  CPU cores (default: 1)"
        echo "  MEMORY               Memory in GB (default: 1.5)"
        ;;
    *)
        deploy
        ;;
esac
