#!/bin/bash
# Extract the AppControl CA certificate and provide instructions to trust it.
# This allows browsers to trust the self-signed certificates used in dev.

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CA_FILE="$SCRIPT_DIR/appcontrol-ca.crt"

echo "=== AppControl CA Trust Helper ==="
echo ""

# Extract CA from Docker volume (try both compose files)
echo "Extracting CA certificate from Docker..."
EXTRACTED=false

for COMPOSE_FILE in docker-compose.release.yaml docker-compose.yaml docker-compose.prod.yaml; do
    if [ -f "$SCRIPT_DIR/$COMPOSE_FILE" ] && docker compose -f "$SCRIPT_DIR/$COMPOSE_FILE" ps nginx 2>/dev/null | grep -q "running"; then
        docker compose -f "$SCRIPT_DIR/$COMPOSE_FILE" cp nginx:/certs/ca.crt "$CA_FILE" 2>/dev/null && EXTRACTED=true && break
    fi
done

# Fallback: try common container names directly
if [ "$EXTRACTED" = false ]; then
    docker cp docker-nginx-1:/certs/ca.crt "$CA_FILE" 2>/dev/null && EXTRACTED=true || \
    docker cp appcontrol-nginx-1:/certs/ca.crt "$CA_FILE" 2>/dev/null && EXTRACTED=true
fi

if [ "$EXTRACTED" = false ]; then
    echo "ERROR: Could not extract CA. Make sure containers are running:"
    echo "  docker compose -f docker/docker-compose.release.yaml up -d"
    exit 1
fi

if [ ! -f "$CA_FILE" ]; then
    echo "ERROR: Failed to extract CA certificate"
    exit 1
fi

echo "CA certificate saved to: $CA_FILE"
echo ""

# Show fingerprint
echo "CA Fingerprint (SHA-256):"
openssl x509 -in "$CA_FILE" -noout -fingerprint -sha256 2>/dev/null | sed 's/.*=/  /'
echo ""

# OS-specific instructions
OS="$(uname -s)"
case "$OS" in
    Darwin)
        echo "=== macOS Instructions ==="
        echo ""
        echo "Option 1: Automatic (requires sudo)"
        echo "  sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain '$CA_FILE'"
        echo ""
        echo "Option 2: Manual"
        echo "  1. Double-click the certificate file (opens Keychain Access)"
        echo "  2. Find 'AppControl CA' in the list"
        echo "  3. Double-click it, expand 'Trust', set 'Always Trust'"
        echo ""
        read -p "Open Keychain Access now? [y/N] " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            open "$CA_FILE"
        fi
        ;;
    Linux)
        echo "=== Linux Instructions ==="
        echo ""
        echo "For system-wide trust (Debian/Ubuntu):"
        echo "  sudo cp '$CA_FILE' /usr/local/share/ca-certificates/appcontrol-ca.crt"
        echo "  sudo update-ca-certificates"
        echo ""
        echo "For system-wide trust (RHEL/CentOS/Fedora):"
        echo "  sudo cp '$CA_FILE' /etc/pki/ca-trust/source/anchors/appcontrol-ca.crt"
        echo "  sudo update-ca-trust"
        echo ""
        echo "For Firefox only:"
        echo "  Settings > Privacy & Security > Certificates > View Certificates"
        echo "  > Authorities > Import > select '$CA_FILE'"
        echo ""
        echo "For Chrome/Chromium:"
        echo "  Settings > Privacy and Security > Security > Manage certificates"
        echo "  > Authorities > Import > select '$CA_FILE'"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        echo "=== Windows Instructions ==="
        echo ""
        echo "Option 1: Command line (run as Administrator)"
        echo "  certutil -addstore -f \"ROOT\" \"$CA_FILE\""
        echo ""
        echo "Option 2: Manual"
        echo "  1. Double-click the certificate file"
        echo "  2. Click 'Install Certificate'"
        echo "  3. Select 'Local Machine', click Next"
        echo "  4. Select 'Place all certificates in the following store'"
        echo "  5. Browse, select 'Trusted Root Certification Authorities'"
        echo "  6. Click Next, then Finish"
        ;;
    *)
        echo "Unknown OS: $OS"
        echo "Please manually import '$CA_FILE' into your browser's certificate store."
        ;;
esac

echo ""
echo "After importing, restart your browser and access https://localhost"
