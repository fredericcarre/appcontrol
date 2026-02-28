#!/usr/bin/env bash
# =============================================================================
# AppControl — OpenShift Developer Sandbox Deployment
# =============================================================================
# Usage: ./deploy.sh [--delete]
#
# Prerequisites:
#   1. oc CLI installed (https://mirror.openshift.com/pub/openshift-v4/clients/ocp/latest/)
#   2. Logged in: oc login --token=<token> --server=<api-url>
#   3. GHCR images are public or pull secret is configured
# =============================================================================

set -euo pipefail

# ── Configurable variables (override via environment) ─────────────────────────
POSTGRES_DB="${POSTGRES_DB:-appcontrol}"
POSTGRES_USER="${POSTGRES_USER:-appcontrol}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-$(openssl rand -base64 18)}"
JWT_SECRET="${JWT_SECRET:-$(openssl rand -base64 48)}"

SEED_ADMIN_EMAIL="${SEED_ADMIN_EMAIL:-admin@localhost}"
SEED_ADMIN_DISPLAY_NAME="${SEED_ADMIN_DISPLAY_NAME:-Admin}"
SEED_ORG_NAME="${SEED_ORG_NAME:-Default Organization}"
SEED_ORG_SLUG="${SEED_ORG_SLUG:-default}"

IMAGE_TAG="${IMAGE_TAG:-latest}"
IMAGE_REGISTRY="${IMAGE_REGISTRY:-ghcr.io/fredericcarre}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

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

# ── Preflight checks ─────────────────────────────────────────────────────────
check_prerequisites() {
    if ! command -v oc &>/dev/null; then
        error "oc CLI not found. Install from: https://mirror.openshift.com/pub/openshift-v4/clients/ocp/latest/"
        exit 1
    fi

    if ! oc whoami &>/dev/null; then
        error "Not logged in to OpenShift. Run: oc login --token=<token> --server=<api-url>"
        exit 1
    fi

    local project
    project=$(oc project -q 2>/dev/null)
    info "Logged in as $(oc whoami) on project: ${project}"
}

# ── Delete ────────────────────────────────────────────────────────────────────
delete_all() {
    warn "Deleting all AppControl resources..."
    oc delete route appcontrol appcontrol-backend --ignore-not-found=true
    oc delete deployment appcontrol-frontend appcontrol-backend appcontrol-postgres --ignore-not-found=true
    oc delete service appcontrol-frontend appcontrol-backend appcontrol-postgres --ignore-not-found=true
    oc delete configmap appcontrol-frontend-nginx --ignore-not-found=true
    oc delete secret appcontrol-postgres appcontrol-backend --ignore-not-found=true
    oc delete pvc appcontrol-postgres-data --ignore-not-found=true
    ok "All AppControl resources deleted."
    exit 0
}

# ── Substitute env vars in YAML ───────────────────────────────────────────────
apply_template() {
    local file="$1"
    info "Applying ${file}..."
    envsubst < "${SCRIPT_DIR}/${file}" | oc apply -f -
}

# ── Main deployment ──────────────────────────────────────────────────────────
deploy() {
    check_prerequisites

    local project
    project=$(oc project -q)

    # Derive route hosts from project and cluster domain
    # OpenShift sandbox pattern: <name>-<project>.apps.<cluster>
    local cluster_domain
    cluster_domain=$(oc whoami --show-server | sed 's|https://api\.||; s|:.*||')
    ROUTE_HOST="${ROUTE_HOST:-appcontrol-${project}.apps.${cluster_domain}}"
    BACKEND_ROUTE_HOST="${BACKEND_ROUTE_HOST:-appcontrol-api-${project}.apps.${cluster_domain}}"
    CORS_ORIGINS="${CORS_ORIGINS:-https://${ROUTE_HOST}}"

    export POSTGRES_DB POSTGRES_USER POSTGRES_PASSWORD JWT_SECRET
    export SEED_ADMIN_EMAIL SEED_ADMIN_DISPLAY_NAME SEED_ORG_NAME SEED_ORG_SLUG
    export ROUTE_HOST BACKEND_ROUTE_HOST CORS_ORIGINS

    info "============================================"
    info "  AppControl OpenShift Deployment"
    info "============================================"
    info "Project:         ${project}"
    info "Frontend URL:    https://${ROUTE_HOST}"
    info "Backend URL:     https://${BACKEND_ROUTE_HOST}"
    info "Image registry:  ${IMAGE_REGISTRY}"
    info "Image tag:       ${IMAGE_TAG}"
    info "Admin email:     ${SEED_ADMIN_EMAIL}"
    info "============================================"
    echo ""

    # ── Step 1: Secrets ───────────────────────────────────────────────────────
    info "Step 1/6: Creating secrets..."
    apply_template postgres-secret.yaml
    apply_template backend-secret.yaml
    ok "Secrets created."

    # ── Step 2: PostgreSQL ────────────────────────────────────────────────────
    info "Step 2/6: Deploying PostgreSQL..."
    apply_template postgres-pvc.yaml
    apply_template postgres-deployment.yaml
    apply_template postgres-service.yaml
    ok "PostgreSQL deployed."

    info "Waiting for PostgreSQL to be ready..."
    oc rollout status deployment/appcontrol-postgres --timeout=120s

    # ── Step 3: Backend ───────────────────────────────────────────────────────
    info "Step 3/6: Deploying Backend..."

    # Update image in deployment
    export BACKEND_IMAGE="${IMAGE_REGISTRY}/appcontrol-backend:${IMAGE_TAG}"
    sed "s|ghcr.io/fredericcarre/appcontrol-backend:latest|${BACKEND_IMAGE}|g" \
        "${SCRIPT_DIR}/backend-deployment.yaml" | envsubst | oc apply -f -

    apply_template backend-service.yaml
    ok "Backend deployed."

    info "Waiting for Backend to be ready..."
    oc rollout status deployment/appcontrol-backend --timeout=180s

    # ── Step 4: Frontend ──────────────────────────────────────────────────────
    info "Step 4/6: Deploying Frontend..."
    apply_template frontend-configmap.yaml

    export FRONTEND_IMAGE="${IMAGE_REGISTRY}/appcontrol-frontend:${IMAGE_TAG}"
    sed "s|ghcr.io/fredericcarre/appcontrol-frontend:latest|${FRONTEND_IMAGE}|g" \
        "${SCRIPT_DIR}/frontend-deployment.yaml" | envsubst | oc apply -f -

    apply_template frontend-service.yaml
    ok "Frontend deployed."

    info "Waiting for Frontend to be ready..."
    oc rollout status deployment/appcontrol-frontend --timeout=120s

    # ── Step 5: Routes ────────────────────────────────────────────────────────
    info "Step 5/6: Creating Routes..."
    apply_template frontend-route.yaml
    apply_template backend-route.yaml
    ok "Routes created."

    # ── Step 6: Summary ───────────────────────────────────────────────────────
    echo ""
    info "============================================"
    ok "  Deployment complete!"
    info "============================================"
    echo ""
    echo -e "  Frontend:  ${GREEN}https://${ROUTE_HOST}${NC}"
    echo -e "  Backend:   ${GREEN}https://${BACKEND_ROUTE_HOST}${NC}"
    echo -e "  Health:    ${GREEN}https://${BACKEND_ROUTE_HOST}/health${NC}"
    echo ""
    echo -e "  Login:     ${YELLOW}${SEED_ADMIN_EMAIL}${NC} (leave password empty)"
    echo ""
    echo -e "  Gateway WS URL (for Azure gateway): ${BLUE}wss://${BACKEND_ROUTE_HOST}/ws/gateway${NC}"
    echo ""
    echo -e "  ${YELLOW}Save these credentials — they won't be shown again:${NC}"
    echo -e "  POSTGRES_PASSWORD=${POSTGRES_PASSWORD}"
    echo -e "  JWT_SECRET=${JWT_SECRET}"
    echo ""
}

# ── Entry point ───────────────────────────────────────────────────────────────
case "${1:-}" in
    --delete|--teardown|--remove)
        check_prerequisites
        delete_all
        ;;
    --help|-h)
        echo "Usage: $0 [--delete]"
        echo ""
        echo "Environment variables:"
        echo "  POSTGRES_PASSWORD   PostgreSQL password (auto-generated if not set)"
        echo "  JWT_SECRET          JWT signing key (auto-generated if not set)"
        echo "  IMAGE_TAG           Docker image tag (default: latest)"
        echo "  IMAGE_REGISTRY      Docker image registry (default: ghcr.io/fredericcarre)"
        echo "  SEED_ADMIN_EMAIL    Admin email (default: admin@localhost)"
        echo "  ROUTE_HOST          Frontend route hostname (auto-derived)"
        echo "  BACKEND_ROUTE_HOST  Backend route hostname (auto-derived)"
        ;;
    *)
        deploy
        ;;
esac
