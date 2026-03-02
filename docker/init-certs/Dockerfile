# AppControl certificate initialization container
# Runs once at startup to initialize TLS certificates

FROM alpine:3.21

# Upgrade base packages to fix known vulnerabilities, then install required tools
RUN apk upgrade --no-cache && \
    apk add --no-cache \
        curl \
        openssl \
        ca-certificates

# Copy initialization script
COPY init-certs.sh /init-certs.sh
RUN chmod +x /init-certs.sh

# Note: Run as root to write to the Docker volume
# For OpenShift, the volume should be pre-created with correct permissions
# or use a PVC with the appropriate security context

# Default environment
ENV CERT_DIR=/certs \
    BACKEND_URL=http://backend:3000 \
    SERVER_CN=localhost \
    VALIDITY_DAYS=365

# The script exits after initialization
CMD ["/init-certs.sh"]
