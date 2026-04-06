# AppControl Metrics Demo - Setup Script
# Creates JSON metrics files used by the demo health checks.
# Run once before importing metrics-demo-windows.json.

$dir = "$env:TEMP\appcontrol_demo"
New-Item -ItemType Directory -Force -Path $dir | Out-Null

# PostgreSQL Database
Set-Content -Path "$dir\postgres-db.json" -Value '{"connections": 42, "replication_lag_ms": 150, "cache_hit_ratio": 98, "cache_hit_ratio_widget": "gauge", "database_size_mb": 2048, "version": "PostgreSQL 16.2", "version_widget": "text"}' -Encoding ASCII

# Redis Cache
Set-Content -Path "$dir\redis-cache.json" -Value '{"memory_percent": 64, "memory_percent_widget": "gauge", "keys": 15420, "hit_rate": 92, "hit_rate_widget": "gauge", "status": "ok", "status_widget": "status"}' -Encoding ASCII

# Kafka Broker
Set-Content -Path "$dir\kafka-broker.json" -Value '{"topics": 12, "partitions": 48, "consumer_lag": 1250, "messages_per_sec": [5420, 5380, 5510, 5440], "messages_per_sec_widget": "sparkline", "broker_status": "ok", "broker_status_widget": "status"}' -Encoding ASCII

# API Gateway
Set-Content -Path "$dir\api-gateway.json" -Value '{"requests_per_min": 12500, "error_rate": 0.2, "error_rate_widget": "gauge", "p99_latency_ms": 45, "active_connections": 320, "backends": {"api": 95, "auth": 100, "static": 100}, "backends_widget": "bars"}' -Encoding ASCII

# Backend API
Set-Content -Path "$dir\backend-api.json" -Value '{"active_users": 145, "orders_today": 523, "avg_response_ms": 28, "threads": {"active": 12, "idle": 38, "blocked": 2}, "threads_widget": "pie", "heap_percent": 65, "heap_percent_widget": "gauge"}' -Encoding ASCII

# Background Workers
Set-Content -Path "$dir\worker-pool.json" -Value '{"workers_active": 8, "workers_active_widget": "number", "queue_depth": 42, "queue_depth_widget": "bar", "jobs_last_hour": [150, 142, 168, 155], "jobs_last_hour_widget": "sparkline", "job_status": "warning", "job_status_widget": "status"}' -Encoding ASCII

# Frontend App
Set-Content -Path "$dir\frontend.json" -Value '{"requests_per_sec": 850, "bandwidth_mbps": 12.5, "cache_hit_ratio": 95, "cache_hit_ratio_widget": "gauge", "top_pages": [{"path": "/", "hits": 1200}, {"path": "/products", "hits": 890}], "top_pages_widget": "table"}' -Encoding ASCII

Write-Host "Metrics demo setup complete. Files created in $dir"
Write-Host "Now import metrics-demo-windows.json and run Start All."
