# CLAUDE.md - crates/gateway

## Purpose
Network zone relay. Allows agents in isolated zones (DMZ, partner networks) to connect to backend without direct network access. Single binary ~6MB.

## Dependencies
Same as agent minus sled/sysinfo. Add: axum (for WSS server accepting agents).

## Architecture
```
gateway/src/
├── main.rs            # CLI, config, start WSS server + WSS client
├── registry.rs        # Track connected agents (id, hostname, last_heartbeat)
├── router.rs          # Route messages: agent↔backend bidirectional
├── rate_limit.rs      # Rate limiting for gateway connections
└── win_service.rs     # Windows service support
```

## Key Behavior
- 1 gateway per network zone (PRD, DMZ, DR)
- Accepts agent connections on port 443 (WSS with mTLS)
- Forwards all messages to backend transparently
- Maintains agent registry: reports connected agents count to backend
- If backend disconnects: buffer messages (small in-memory buffer, 10MB max), reconnect
- If agent disconnects: remove from registry, notify backend

## Configuration (gateway.yaml)
```yaml
gateway:
  id: gateway-prd-01
  zone: PRD
  listen_addr: 0.0.0.0
  listen_port: 443
backend:
  url: wss://backend.internal:443/gateway
  reconnect_interval_secs: 5
tls:                                          # Agent-facing mTLS (server)
  cert_file: /etc/appcontrol/certs/gateway.crt
  key_file: /etc/appcontrol/certs/gateway.key
  ca_file: /etc/appcontrol/certs/ca.crt
  verify_clients: true
backend_tls:                                  # Backend-facing TLS (client)
  ca_file: /etc/appcontrol/certs/backend-ca.crt   # Internal PKI CA (optional, defaults to system roots)
  cert_file: /etc/appcontrol/certs/gateway.crt    # Client cert for mTLS to backend (optional)
  key_file: /etc/appcontrol/certs/gateway.key      # Client key for mTLS to backend (optional)
```

### Environment Variables
| Variable | Description |
|----------|-------------|
| `BACKEND_URL` | WebSocket URL to backend (`wss://` in production) |
| `BACKEND_TLS_CA_FILE` | CA certificate to verify backend (internal PKI) |
| `BACKEND_TLS_CERT_FILE` | Client certificate for mTLS to backend |
| `BACKEND_TLS_KEY_FILE` | Client key for mTLS to backend |
| `TLS_ENABLED` | Enable mTLS for agent connections |
| `TLS_CERT_FILE` | Server certificate for agent connections |
| `TLS_KEY_FILE` | Server key for agent connections |
| `TLS_CA_FILE` | CA certificate to verify agent certificates |

## Tests
- Agent connects, message forwarded to backend
- Backend sends command, forwarded to correct agent
- Agent disconnects, registry updated, backend notified
- Backend disconnects, messages buffered, replayed on reconnect
