#!/bin/sh
# Initialize TLS certificates for nginx
#
# This script:
# 1. Waits for the backend to be healthy
# 2. Fetches the CA certificate from the backend
# 3. Generates a self-signed server certificate if one doesn't exist
# 4. Writes certificates to the shared /certs volume

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

# Try to fetch CA from backend
echo "Fetching CA certificate from backend..."
CA_RESPONSE=$(curl -sf "$BACKEND_URL/api/v1/pki/ca-public" 2>/dev/null || echo "")

if [ -n "$CA_RESPONSE" ] && echo "$CA_RESPONSE" | grep -q "ca_cert_pem"; then
    echo "CA certificate available from backend"

    # Extract CA cert from JSON response
    echo "$CA_RESPONSE" | sed -n 's/.*"ca_cert_pem":"\([^"]*\)".*/\1/p' | \
        sed 's/\\n/\n/g' > "$CERT_DIR/ca.crt"

    if [ -s "$CERT_DIR/ca.crt" ]; then
        echo "CA certificate saved to $CERT_DIR/ca.crt"
    else
        echo "WARNING: Could not extract CA certificate, generating self-signed"
        GENERATE_SELF_SIGNED=true
    fi
else
    echo "No CA certificate available from backend (PKI not initialized)"
    echo "Will generate self-signed certificates"
    GENERATE_SELF_SIGNED=true
fi

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

# Generate self-signed certificates if needed
if [ "$GENERATE_SELF_SIGNED" = "true" ] || [ ! -f "$CERT_DIR/ca.crt" ]; then
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

# Cleanup temporary files
rm -f /tmp/server.cnf /tmp/server.csr "$CERT_DIR/ca.key" "$CERT_DIR/ca.srl" 2>/dev/null || true

# Verify certificates
echo ""
echo "=== Certificate Summary ==="
echo "CA Certificate:"
openssl x509 -in "$CERT_DIR/ca.crt" -noout -subject -issuer -dates 2>/dev/null

echo ""
echo "Server Certificate:"
openssl x509 -in "$CERT_DIR/server.crt" -noout -subject -issuer -dates 2>/dev/null

echo ""
echo "Server Certificate SANs:"
openssl x509 -in "$CERT_DIR/server.crt" -noout -text 2>/dev/null | grep -A1 "Subject Alternative Name"

echo ""
echo "=== Certificate initialization complete ==="
