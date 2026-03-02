# AppControl certificate initialization container
# Runs once at startup to initialize TLS certificates

FROM alpine:3.20

# Install required tools
RUN apk add --no-cache \
    curl \
    openssl \
    ca-certificates

# Copy initialization script
COPY init-certs.sh /init-certs.sh
RUN chmod +x /init-certs.sh

# Run as non-root (OpenShift compatible)
RUN adduser -D -u 1000 certbot
USER certbot

# Default environment
ENV CERT_DIR=/certs \
    BACKEND_URL=http://backend:3000 \
    SERVER_CN=localhost \
    VALIDITY_DAYS=365

# The script exits after initialization
CMD ["/init-certs.sh"]
