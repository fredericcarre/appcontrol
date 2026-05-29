#!/usr/bin/env python3
"""A tiny stand-in for a Spring-Boot 'order-api'.

It exists to give the AppControl agent something realistic to discover:
  * it keeps its config file (application.yml) open  -> agent finds it via /proc/fd
    and extracts the PostgreSQL/Redis endpoints from it (config-based dependencies),
  * it holds open TCP connections to PostgreSQL (5432) and Redis (6379)
    -> agent observes them as runtime dependencies,
  * it listens on :8080                              -> agent types it as a service.
"""
import http.server
import socket
import time

CONFIG_PATH = "/opt/order-api/config/application.yml"

# Keep the config file descriptor open for the lifetime of the process.
_config_fd = open(CONFIG_PATH, "r")  # noqa: SIM115 (intentionally kept open)

# Hold open connections to the backing services so they show up as dependencies.
_open_conns = []
for port in (5432, 6379):
    for _ in range(60):
        try:
            _open_conns.append(socket.create_connection(("127.0.0.1", port), timeout=1))
            break
        except OSError:
            time.sleep(1)


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):  # noqa: N802
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"order-api ok")

    def log_message(self, *_args):
        pass


if __name__ == "__main__":
    http.server.HTTPServer(("0.0.0.0", 8080), Handler).serve_forever()
