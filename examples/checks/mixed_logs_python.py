#!/usr/bin/env python3
"""Health check emitting verbose logs followed by a single JSON metrics line.

Demonstrates the auto-detect format: AppControl scans stdout backwards for
the last JSON object and uses it as the metrics payload. The verbose log
output remains visible in the check_events.stdout column for debugging.
"""
import json
import random
import sys
import time

print(f"[{time.strftime('%H:%M:%S')}] Connecting to database...")
print(f"[{time.strftime('%H:%M:%S')}] Connected (took 12ms)")
print(f"[{time.strftime('%H:%M:%S')}] Running SELECT count(*) FROM orders WHERE status='pending'")

pending = random.randint(0, 500)
oldest_age_s = random.randint(0, 7200)

print(f"[{time.strftime('%H:%M:%S')}] Found {pending} pending orders")
print(f"[{time.strftime('%H:%M:%S')}] Oldest pending: {oldest_age_s}s")

print(json.dumps({
    "pending_orders": pending,
    "oldest_age_s": oldest_age_s,
}))

sys.exit(2 if oldest_age_s > 3600 else 0)
