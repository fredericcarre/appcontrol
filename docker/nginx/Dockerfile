# AppControl nginx with TLS termination
# Based on nginx:alpine with stream module for mTLS passthrough

FROM nginx:1.27-alpine

# Install stream module for TCP passthrough
RUN apk add --no-cache nginx-mod-stream

# Copy configuration
COPY nginx.conf /etc/nginx/nginx.conf

# Create non-root user directories
RUN mkdir -p /var/cache/nginx /var/log/nginx /tmp && \
    chown -R nginx:nginx /var/cache/nginx /var/log/nginx /tmp && \
    chmod -R 755 /var/cache/nginx /var/log/nginx

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget -q --spider http://localhost/health || exit 1

# Expose ports
# 80: HTTP (redirect to HTTPS)
# 443: HTTPS (TLS termination)
# 8443: mTLS passthrough to gateway
EXPOSE 80 443 8443

# Run as nginx user (OpenShift compatible with arbitrary UIDs)
USER nginx

CMD ["nginx", "-g", "daemon off;"]
