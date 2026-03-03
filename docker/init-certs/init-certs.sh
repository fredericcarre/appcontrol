#!/bin/sh
# Initialize TLS certificates for nginx and gateway
#
# This script:
# 1. Waits for the backend to be healthy
# 2. Generates a self-signed CA for nginx TLS (browser HTTPS)
# 3. Generates a nginx server certificate
# 4. Fetches the PKI CA from the backend (for mTLS)
# 5. Generates a gateway certificate signed by the PKI CA
# 6. Writes all certificates to the shared /certs volume

set -e

CERT_DIR="${CERT_DIR:-/certs}"
BACKEND_URL="${BACKEND_URL:-http://backend:3000}"
SERVER_CN="${SERVER_CN:-localhost}"
VALIDITY_DAYS="${VALIDITY_DAYS:-365}"

echo "=== AppControl Certificate Initialization ==="
echo "Backend URL: $BACKEND_URL"
echo "Certificate directory: $CERT_DIR"
echo "Server CN: $SERVER_CN"

# Create cert directory
mkdir -p "$CERT_DIR"

# Wait for backend to be healthy
echo "Waiting for backend to be ready..."
MAX_RETRIES=60
RETRY_INTERVAL=2
retries=0

while [ $retries -lt $MAX_RETRIES ]; do
    if curl -sf "$BACKEND_URL/health" > /dev/null 2>&1; then
        echo "Backend is healthy"
        break
    fi
    retries=$((retries + 1))
    echo "Waiting for backend... ($retries/$MAX_RETRIES)"
    sleep $RETRY_INTERVAL
done

if [ $retries -eq $MAX_RETRIES ]; then
    echo "ERROR: Backend did not become healthy within timeout"
    exit 1
fi

# Give the backend a moment to complete PKI initialization
sleep 2

# Note: For nginx TLS termination, we generate a self-signed CA locally.
# The backend's mTLS CA is separate and used for agent/gateway authentication.
# This keeps the nginx TLS setup simple and independent.
echo "Generating self-signed CA for nginx TLS..."
GENERATE_SELF_SIGNED=true

# Check if server cert already exists and is valid
if [ -f "$CERT_DIR/server.crt" ] && [ -f "$CERT_DIR/server.key" ]; then
    # Check if cert is still valid (not expired)
    if openssl x509 -checkend 86400 -noout -in "$CERT_DIR/server.crt" 2>/dev/null; then
        echo "Valid server certificate already exists, skipping generation"

        # Verify the key matches the cert
        CERT_MODULUS=$(openssl x509 -noout -modulus -in "$CERT_DIR/server.crt" 2>/dev/null | md5sum)
        KEY_MODULUS=$(openssl rsa -noout -modulus -in "$CERT_DIR/server.key" 2>/dev/null | md5sum)

        if [ "$CERT_MODULUS" = "$KEY_MODULUS" ]; then
            echo "Certificate and key match"
            echo "=== Certificate initialization complete ==="
            exit 0
        else
            echo "WARNING: Certificate and key do not match, regenerating"
        fi
    else
        echo "Server certificate expired or will expire soon, regenerating"
    fi
fi

# Generate self-signed CA for nginx TLS
if [ ! -f "$CERT_DIR/ca.crt" ] || [ ! -f "$CERT_DIR/ca.key" ]; then
    echo "Generating self-signed CA..."

    # Generate CA key
    openssl genrsa -out "$CERT_DIR/ca.key" 4096 2>/dev/null

    # Generate CA certificate
    openssl req -x509 -new -nodes \
        -key "$CERT_DIR/ca.key" \
        -sha256 \
        -days 3650 \
        -out "$CERT_DIR/ca.crt" \
        -subj "/CN=AppControl CA/O=AppControl/C=US" \
        2>/dev/null

    echo "Self-signed CA generated"
fi

# Generate server certificate
echo "Generating server certificate for CN=$SERVER_CN..."

# Create OpenSSL config for SANs
cat > /tmp/server.cnf << EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
CN = $SERVER_CN
O = AppControl
C = US

[v3_req]
keyUsage = critical, digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = @alt_names

[alt_names]
DNS.1 = $SERVER_CN
DNS.2 = localhost
DNS.3 = nginx
DNS.4 = appcontrol
IP.1 = 127.0.0.1
EOF

# Add extra SANs if provided
if [ -n "$EXTRA_SANS" ]; then
    echo "Adding extra SANs: $EXTRA_SANS"
    san_index=5
    ip_index=2
    for san in $(echo "$EXTRA_SANS" | tr ',' ' '); do
        if echo "$san" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$'; then
            echo "IP.$ip_index = $san" >> /tmp/server.cnf
            ip_index=$((ip_index + 1))
        else
            echo "DNS.$san_index = $san" >> /tmp/server.cnf
            san_index=$((san_index + 1))
        fi
    done
fi

# Generate server key
openssl genrsa -out "$CERT_DIR/server.key" 2048 2>/dev/null

# Generate CSR
openssl req -new \
    -key "$CERT_DIR/server.key" \
    -out /tmp/server.csr \
    -config /tmp/server.cnf \
    2>/dev/null

# Sign with CA
openssl x509 -req \
    -in /tmp/server.csr \
    -CA "$CERT_DIR/ca.crt" \
    -CAkey "$CERT_DIR/ca.key" \
    -CAcreateserial \
    -out "$CERT_DIR/server.crt" \
    -days "$VALIDITY_DAYS" \
    -sha256 \
    -extensions v3_req \
    -extfile /tmp/server.cnf \
    2>/dev/null

# Set permissions
chmod 644 "$CERT_DIR/ca.crt" "$CERT_DIR/server.crt"
chmod 600 "$CERT_DIR/server.key"

# Cleanup temporary files (keep ca.key for future renewals)
chmod 600 "$CERT_DIR/ca.key"
rm -f /tmp/server.cnf /tmp/server.csr "$CERT_DIR/ca.srl" 2>/dev/null || true

# Verify nginx certificates
echo ""
echo "=== Nginx Certificate Summary ==="
echo "CA Certificate:"
openssl x509 -in "$CERT_DIR/ca.crt" -noout -subject -issuer -dates 2>/dev/null

echo ""
echo "Server Certificate:"
openssl x509 -in "$CERT_DIR/server.crt" -noout -subject -issuer -dates 2>/dev/null

echo ""
echo "Server Certificate SANs:"
openssl x509 -in "$CERT_DIR/server.crt" -noout -text 2>/dev/null | grep -A1 "Subject Alternative Name"

# Fetch PKI CA and generate gateway certificate via API
echo ""
echo "=== Fetching PKI certificates for gateway mTLS ==="

# Get the PKI CA public certificate (unauthenticated endpoint)
PKI_CA_RESPONSE=$(curl -sf "$BACKEND_URL/api/v1/pki/ca-public" 2>/dev/null || echo "")

if [ -n "$PKI_CA_RESPONSE" ] && echo "$PKI_CA_RESPONSE" | grep -q "ca_cert_pem"; then
    echo "PKI CA retrieved from backend"

    # Extract CA cert and save it
    echo "$PKI_CA_RESPONSE" | sed -n 's/.*"ca_cert_pem":"\([^"]*\)".*/\1/p' | sed 's/\\n/\n/g' > "$CERT_DIR/pki-ca.crt"

    # Authenticate to get a token for server cert issuance
    ADMIN_EMAIL="${SEED_ADMIN_EMAIL:-admin@localhost}"
    ADMIN_PASSWORD="${SEED_ADMIN_PASSWORD:-admin}"

    echo "Authenticating to generate gateway certificate..."
    AUTH_RESPONSE=$(curl -sf -X POST "$BACKEND_URL/api/v1/auth/login" \
        -H "Content-Type: application/json" \
        -d "{\"email\":\"$ADMIN_EMAIL\",\"password\":\"$ADMIN_PASSWORD\"}" 2>/dev/null || echo "")

    if echo "$AUTH_RESPONSE" | grep -q "token"; then
        TOKEN=$(echo "$AUTH_RESPONSE" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')

        # Request a server certificate for the gateway
        GATEWAY_CN="${GATEWAY_CN:-gateway}"
        CERT_RESPONSE=$(curl -sf -X POST "$BACKEND_URL/api/v1/pki/server-cert" \
            -H "Content-Type: application/json" \
            -H "Authorization: Bearer $TOKEN" \
            -d "{\"common_name\":\"$GATEWAY_CN\",\"san_dns\":[\"gateway\",\"localhost\",\"appcontrol-gateway\"],\"san_ips\":[\"127.0.0.1\"],\"validity_days\":365}" 2>/dev/null || echo "")

        if echo "$CERT_RESPONSE" | grep -q "cert_pem"; then
            echo "Gateway certificate generated"

            # Extract and save certificate and key
            echo "$CERT_RESPONSE" | sed -n 's/.*"cert_pem":"\([^"]*\)".*/\1/p' | sed 's/\\n/\n/g' > "$CERT_DIR/gateway.crt"
            echo "$CERT_RESPONSE" | sed -n 's/.*"key_pem":"\([^"]*\)".*/\1/p' | sed 's/\\n/\n/g' > "$CERT_DIR/gateway.key"
            # Make readable by gateway container (runs as non-root user)
            chmod 644 "$CERT_DIR/gateway.crt" "$CERT_DIR/gateway.key"

            echo ""
            echo "=== PKI Certificate Summary ==="
            echo "PKI CA Certificate:"
            openssl x509 -in "$CERT_DIR/pki-ca.crt" -noout -subject -issuer -dates 2>/dev/null
            echo ""
            echo "Gateway Certificate:"
            openssl x509 -in "$CERT_DIR/gateway.crt" -noout -subject -issuer -dates 2>/dev/null
        else
            echo "WARNING: Failed to generate gateway certificate: $CERT_RESPONSE"
        fi
    else
        echo "WARNING: Failed to authenticate for gateway cert generation"
    fi
else
    echo "WARNING: PKI not initialized. Gateway mTLS will not work."
    echo "Initialize PKI via the web UI or API first."
fi

echo ""
echo "=== Certificate initialization complete ==="
