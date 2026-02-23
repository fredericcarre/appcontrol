#!/usr/bin/env bash
# AppControl - Local Development Setup (macOS)
# Usage: ./docker/dev-setup.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.dev.yaml"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info()  { echo -e "${BLUE}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERR]${NC}   $*"; }

echo ""
echo "=========================================="
echo "  AppControl v4 - Dev Environment Setup"
echo "=========================================="
echo ""

# ---- Check prerequisites ----
info "Checking prerequisites..."

check_cmd() {
    if command -v "$1" &>/dev/null; then
        ok "$1 found: $($1 --version 2>&1 | head -1)"
    else
        err "$1 not found. Install with: $2"
        return 1
    fi
}

MISSING=0
check_cmd docker "brew install --cask docker" || MISSING=1
check_cmd cargo  "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" || MISSING=1
check_cmd node   "brew install node@22" || MISSING=1
check_cmd npm    "brew install node@22" || MISSING=1

if [ "$MISSING" -eq 1 ]; then
    err "Missing prerequisites. Install them and re-run this script."
    exit 1
fi

# Check Docker is running
if ! docker info &>/dev/null; then
    err "Docker Desktop is not running. Start it and try again."
    exit 1
fi
ok "Docker Desktop is running"

# ---- Start infrastructure ----
info "Starting PostgreSQL 16..."
docker compose -f "$COMPOSE_FILE" up -d postgres

info "Waiting for PostgreSQL to be ready..."
for i in $(seq 1 30); do
    if docker compose -f "$COMPOSE_FILE" exec -T postgres pg_isready -U appcontrol &>/dev/null; then
        ok "PostgreSQL is ready"
        break
    fi
    if [ "$i" -eq 30 ]; then
        err "PostgreSQL failed to start within 30s"
        exit 1
    fi
    sleep 1
done

# ---- Run database migrations ----
info "Running database migrations..."
if command -v sqlx &>/dev/null; then
    DATABASE_URL="postgres://appcontrol:appcontrol_dev@localhost:5432/appcontrol" \
        sqlx migrate run --source "$PROJECT_DIR/migrations/"
    ok "Migrations applied"
else
    warn "sqlx-cli not installed. Installing..."
    cargo install sqlx-cli --no-default-features --features postgres,rustls
    DATABASE_URL="postgres://appcontrol:appcontrol_dev@localhost:5432/appcontrol" \
        sqlx migrate run --source "$PROJECT_DIR/migrations/"
    ok "Migrations applied"
fi

# ---- Build Rust workspace ----
info "Building Rust workspace (this may take a few minutes on first run)..."
cd "$PROJECT_DIR"
cargo build --workspace
ok "Rust workspace built"

# ---- Install frontend dependencies ----
info "Installing frontend dependencies..."
cd "$PROJECT_DIR/frontend"
npm ci
ok "Frontend dependencies installed"

# ---- Summary ----
echo ""
echo "=========================================="
echo "  Dev environment is ready!"
echo "=========================================="
echo ""
echo "  Infrastructure:"
echo "    PostgreSQL : localhost:5432 (appcontrol/appcontrol_dev)"
echo ""
echo "  Start developing:"
echo ""
echo "    # Terminal 1 - Backend API"
echo "    export DATABASE_URL=postgres://appcontrol:appcontrol_dev@localhost:5432/appcontrol"
echo "    export JWT_SECRET=dev-secret-change-in-production"
echo "    export RUST_LOG=info,appcontrol_backend=debug"
echo "    cargo run --bin appcontrol-backend"
echo ""
echo "    # Terminal 2 - Frontend (hot-reload)"
echo "    cd frontend && npm run dev"
echo ""
echo "    # Terminal 3 - Gateway"
echo "    export RUST_LOG=info,appcontrol_gateway=debug"
echo "    cargo run --bin appcontrol-gateway"
echo ""
echo "    # Terminal 4 - Agent (optional)"
echo "    cargo run --bin appcontrol-agent -- --gateway-url wss://localhost:4443 --name dev-agent"
echo ""
echo "  Optional tools:"
echo "    docker compose -f docker/docker-compose.dev.yaml --profile tools up -d"
echo "    pgAdmin   : http://localhost:5050 (admin@appcontrol.local / admin)"
echo ""
echo "  Tear down:"
echo "    docker compose -f docker/docker-compose.dev.yaml down       # keep data"
echo "    docker compose -f docker/docker-compose.dev.yaml down -v    # wipe data"
echo ""
