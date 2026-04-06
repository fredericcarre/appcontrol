# AppControl Metrics Demo - Setup Script
# Creates JSON metrics files used by the demo health checks.
# Run once before importing metrics-demo-windows.json.

$dir = "$env:TEMP\appcontrol_demo"
New-Item -ItemType Directory -Force -Path $dir | Out-Null

# PostgreSQL Database
@'
{"connections": 42, "replication_lag_ms": 150, "cache_hit_ratio": 98, "cache_hit_ratio_widget": "gauge", "database_size_mb": 2048, "version": "PostgreSQL 16.2", "version_widget": "text"}
'@ | Out-File -Encoding UTF8 "$dir\postgres-db.json"

# Redis Cache
@'
{"memory_percent": 64, "memory_percent_widget": "gauge", "keys": 15420, "hit_rate": 92, "hit_rate_widget": "gauge", "status": "ok", "status_widget": "status"}
'@ | Out-File -Encoding UTF8 "$dir\redis-cache.json"

# Kafka Broker
@'
{"topics": 12, "partitions": 48, "consumer_lag": 1250, "messages_per_sec": [5420, 5380, 5510, 5440], "messages_per_sec_widget": "sparkline", "broker_status": "ok", "broker_status_widget": "status"}
'@ | Out-File -Encoding UTF8 "$dir\kafka-broker.json"

# API Gateway
@'
{"requests_per_min": 12500, "error_rate": 0.2, "error_rate_widget": "gauge", "p99_latency_ms": 45, "active_connections": 320, "backends": {"api": 95, "auth": 100, "static": 100}, "backends_widget": "bars"}
'@ | Out-File -Encoding UTF8 "$dir\api-gateway.json"

# Backend API
@'
{"active_users": 145, "orders_today": 523, "avg_response_ms": 28, "threads": {"active": 12, "idle": 38, "blocked": 2}, "threads_widget": "pie", "heap_percent": 65, "heap_percent_widget": "gauge"}
'@ | Out-File -Encoding UTF8 "$dir\backend-api.json"

# Background Workers
@'
{"workers_active": 8, "workers_active_widget": "number", "queue_depth": 42, "queue_depth_widget": "bar", "jobs_last_hour": [150, 142, 168, 155], "jobs_last_hour_widget": "sparkline", "job_status": "warning", "job_status_widget": "status"}
'@ | Out-File -Encoding UTF8 "$dir\worker-pool.json"

# Frontend App
@'
{"requests_per_sec": 850, "bandwidth_mbps": 12.5, "cache_hit_ratio": 95, "cache_hit_ratio_widget": "gauge", "top_pages": [{"path": "/", "hits": 1200}, {"path": "/products", "hits": 890}], "top_pages_widget": "table"}
'@ | Out-File -Encoding UTF8 "$dir\frontend.json"

Write-Host "Metrics demo setup complete. Files created in $dir"
Write-Host "Now import metrics-demo-windows.json and run Start All."
