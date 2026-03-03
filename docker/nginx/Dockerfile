# AppControl nginx with TLS termination
# Standard nginx image - agents connect directly to gateways, NOT through nginx

FROM nginx:1.27-alpine

# Upgrade base packages to fix known vulnerabilities
RUN apk upgrade --no-cache

# Copy configuration
COPY nginx.conf /etc/nginx/nginx.conf

# Create non-root user directories
RUN mkdir -p /var/cache/nginx /var/log/nginx /tmp && \
    chown -R nginx:nginx /var/cache/nginx /var/log/nginx /tmp && \
    chmod -R 755 /var/cache/nginx /var/log/nginx

# Health check (use 127.0.0.1 instead of localhost for BusyBox wget compatibility)
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget -q --spider http://127.0.0.1/health || exit 1

# Expose ports for web UI only
# 80: HTTP (redirect to HTTPS)
# 443: HTTPS (TLS termination for web UI)
# Note: Agents connect directly to gateways on port 4443, NOT through nginx
EXPOSE 80 443

# Note: Run as root to bind to privileged ports (80, 443)
# For OpenShift, use non-privileged ports (8080, 8443) with a Route

CMD ["nginx", "-g", "daemon off;"]
