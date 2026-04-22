# BRANCH_SPECS.md — Features Unique to This Branch

> Branch: `claude/fix-quickstart-setup-MbgIq`
> Everything else (SQLite dual-db, import wizard, sequencer, discovery, enrollment, gateway failover, E2E tests, sqlx 0.8, etc.) already exists on main.

---

## 1. URL Import Endpoint (`POST /api/v1/import/fetch-url`)

**Does NOT exist on main.** New standalone endpoint.

### Route
```rust
// crates/backend/src/api/mod.rs
.route("/import/fetch-url", post(import::fetch_url))
```

### Request
```json
{ "url": "https://example.com/my-app-map.json" }
```

### Response
```json
{
  "content": "<raw file content as string>",
  "format": "json",
  "content_length": 4523
}
```

### Full Implementation
```rust
// crates/backend/src/api/import.rs

const MAX_FETCH_SIZE: usize = 10 * 1024 * 1024; // 10 MB

#[derive(Debug, Deserialize)]
pub struct FetchUrlRequest {
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct FetchUrlResponse {
    pub content: String,
    pub format: String,
    pub content_length: usize,
}

pub async fn fetch_url(
    State(_state): State<Arc<AppState>>,
    Extension(_user): Extension<AuthUser>,
    Json(body): Json<FetchUrlRequest>,
) -> Result<Json<FetchUrlResponse>, ApiError> {
    let url = body.url.trim().to_string();

    // 1. Validate URL scheme
    let parsed = url::Url::parse(&url)
        .map_err(|e| ApiError::Validation(format!("Invalid URL: {e}")))?;
    match parsed.scheme() {
        "http" | "https" => {}
        s => return Err(ApiError::Validation(format!("Unsupported URL scheme '{s}'. Use http or https."))),
    }

    // 2. Fetch with timeout
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| ApiError::Internal(format!("HTTP client error: {e}")))?;

    let resp = client
        .get(&url)
        .header("User-Agent", "AppControl/4.0")
        .send()
        .await
        .map_err(|e| ApiError::Validation(format!("Failed to fetch URL: {e}")))?;

    if !resp.status().is_success() {
        return Err(ApiError::Validation(format!(
            "URL returned HTTP {}",
            resp.status()
        )));
    }

    // 3. Check Content-Length before reading body
    if let Some(len) = resp.content_length() {
        if len as usize > MAX_FETCH_SIZE {
            return Err(ApiError::Validation(format!(
                "Content too large ({} bytes, max {} bytes)",
                len, MAX_FETCH_SIZE
            )));
        }
    }

    // 4. Read body
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to read response: {e}")))?;

    if bytes.len() > MAX_FETCH_SIZE {
        return Err(ApiError::Validation(format!(
            "Content too large ({} bytes, max {} bytes)",
            bytes.len(),
            MAX_FETCH_SIZE
        )));
    }

    let content = String::from_utf8(bytes.to_vec())
        .map_err(|_| ApiError::Validation("Response is not valid UTF-8".to_string()))?;

    // 5. Auto-detect format
    let format = if content_type.contains("yaml") || content_type.contains("x-yaml") {
        "yaml"
    } else if content_type.contains("json") {
        "json"
    } else {
        let path = parsed.path().to_lowercase();
        if path.ends_with(".yaml") || path.ends_with(".yml") {
            "yaml"
        } else if path.ends_with(".json") {
            "json"
        } else if serde_json::from_str::<serde_json::Value>(&content).is_ok() {
            "json"
        } else {
            "yaml"
        }
    };

    Ok(Json(FetchUrlResponse {
        content_length: content.len(),
        content,
        format: format.to_string(),
    }))
}
```

### Format Detection Priority
1. `Content-Type: application/yaml` or `application/x-yaml` → YAML
2. `Content-Type: application/json` → JSON
3. URL path ends with `.yaml`/`.yml` → YAML
4. URL path ends with `.json` → JSON
5. Body parses as valid JSON → JSON
6. Default → YAML

### Dependency
Requires `reqwest` in `Cargo.toml` (already present on main for other features). Requires `url` crate for URL parsing.

---

## 2. Cluster Example Map Files

**Do NOT exist on main.** Copy directly.

### Files to Add
- `examples/cluster-linux-example.json`
- `examples/cluster-windows-example.json`

### Content: Linux Example
```json
{
  "name": "Linux Cluster Example",
  "description": "Demonstrates cluster mode on Linux with file-based commands.",
  "tags": { "env": "demo", "platform": "linux", "purpose": "cluster-quickstart" },
  "components": [
    {
      "name": "database-cluster",
      "display_name": "Database Cluster",
      "component_type": "database",
      "host": "localhost",
      "cluster_size": 3,
      "cluster_nodes": ["db-primary.local", "db-replica-1.local", "db-replica-2.local"],
      "check_cmd": "test -f /tmp/appcontrol/db-cluster.running",
      "start_cmd": "mkdir -p /tmp/appcontrol && touch /tmp/appcontrol/db-cluster.running",
      "stop_cmd": "rm -f /tmp/appcontrol/db-cluster.running",
      "check_interval_secs": 10,
      "start_timeout_secs": 60,
      "stop_timeout_secs": 30,
      "position": {"x": 400, "y": 400}
    },
    {
      "name": "cache-cluster",
      "display_name": "Cache Cluster",
      "component_type": "cache",
      "host": "localhost",
      "cluster_size": 3,
      "cluster_nodes": ["cache-1.local", "cache-2.local", "cache-3.local"],
      "check_cmd": "test -f /tmp/appcontrol/cache-cluster.running",
      "start_cmd": "mkdir -p /tmp/appcontrol && touch /tmp/appcontrol/cache-cluster.running",
      "stop_cmd": "rm -f /tmp/appcontrol/cache-cluster.running",
      "check_interval_secs": 10,
      "start_timeout_secs": 30,
      "stop_timeout_secs": 15,
      "position": {"x": 200, "y": 250}
    },
    {
      "name": "message-broker",
      "display_name": "Message Broker",
      "component_type": "middleware",
      "host": "localhost",
      "cluster_size": 3,
      "cluster_nodes": ["broker-1.local", "broker-2.local", "broker-3.local"],
      "check_cmd": "test -f /tmp/appcontrol/broker.running",
      "start_cmd": "mkdir -p /tmp/appcontrol && touch /tmp/appcontrol/broker.running",
      "stop_cmd": "rm -f /tmp/appcontrol/broker.running",
      "check_interval_secs": 10,
      "start_timeout_secs": 60,
      "stop_timeout_secs": 30,
      "position": {"x": 600, "y": 250}
    },
    {
      "name": "app-server",
      "display_name": "Application Server",
      "component_type": "application",
      "host": "localhost",
      "cluster_size": 2,
      "cluster_nodes": ["app-1.local", "app-2.local"],
      "check_cmd": "test -f /tmp/appcontrol/app-server.running",
      "start_cmd": "mkdir -p /tmp/appcontrol && touch /tmp/appcontrol/app-server.running",
      "stop_cmd": "rm -f /tmp/appcontrol/app-server.running",
      "check_interval_secs": 10,
      "start_timeout_secs": 30,
      "stop_timeout_secs": 15,
      "position": {"x": 400, "y": 100}
    }
  ],
  "dependencies": [
    {"from": "cache-cluster", "to": "database-cluster"},
    {"from": "message-broker", "to": "database-cluster"},
    {"from": "app-server", "to": "cache-cluster"},
    {"from": "app-server", "to": "message-broker"}
  ]
}
```

### Content: Windows Example
Same topology but with Windows `cmd /c` commands:
- `check_cmd`: `cmd /c if exist C:\\temp\\appcontrol\\<name>.running (exit 0) else (exit 1)`
- `start_cmd`: `cmd /c if not exist C:\\temp\\appcontrol mkdir C:\\temp\\appcontrol & type nul > C:\\temp\\appcontrol\\<name>.running`
- `stop_cmd`: `cmd /c del /f C:\\temp\\appcontrol\\<name>.running 2>nul`
- `cluster_nodes`: Uppercase hostnames (`DB-PRIMARY`, `CACHE-01`, `BROKER-01`, `APP-SVR-01`)

---

## That's It

Only these 2 features are unique to this branch. Everything else already exists on main.
