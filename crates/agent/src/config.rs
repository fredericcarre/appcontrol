use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub agent: AgentSection,
    pub gateway: GatewaySection,
    pub tls: Option<TlsSection>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    /// Log level filter string (e.g. "info", "appcontrol_agent=debug").
    #[serde(default = "default_log_level")]
    pub log_level: Option<String>,
    /// Agent operating mode: "active" (default) or "advisory".
    /// In advisory mode the agent runs health checks and reports state
    /// but refuses to execute start/stop/rebuild commands.
    /// Useful for observation-only deployments during migration.
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Directory for agent data (buffer DB, state files).
    /// Defaults to platform default (/var/lib/appcontrol on Unix, C:\ProgramData\AppControl on Windows).
    /// Can also be set via DATA_DIR environment variable.
    #[serde(default)]
    pub data_dir: Option<String>,
}

fn default_mode() -> String {
    "active".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentSection {
    pub id: String, // "auto" or UUID
}

#[derive(Debug, Deserialize, Clone)]
pub struct GatewaySection {
    /// Single gateway URL (legacy, still supported).
    #[serde(default)]
    pub url: Option<String>,
    /// Multiple gateway URLs for failover (recommended).
    #[serde(default)]
    pub urls: Vec<String>,
    /// Failover strategy: "ordered" (try in order) or "round-robin".
    #[serde(default = "default_failover_strategy")]
    pub failover_strategy: String,
    /// How often (in seconds) to try returning to the primary gateway.
    #[serde(default = "default_primary_retry")]
    pub primary_retry_secs: u64,
    #[serde(default = "default_reconnect_interval")]
    pub reconnect_interval_secs: u64,
    /// Skip TLS certificate verification (for self-signed certs in dev/containers).
    /// WARNING: Do not use in production with untrusted networks.
    #[serde(default)]
    pub tls_insecure: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TlsSection {
    pub enabled: bool,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    pub ca_file: Option<String>,
}

fn default_log_level() -> Option<String> {
    Some("appcontrol_agent=debug".to_string())
}

fn default_reconnect_interval() -> u64 {
    10
}

fn default_failover_strategy() -> String {
    "ordered".to_string()
}

fn default_primary_retry() -> u64 {
    300
}

impl AgentConfig {
    /// Load config from YAML file, then apply env var overrides.
    /// If no config file exists, build entirely from env vars with defaults.
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let mut config = if std::path::Path::new(path).exists() {
            let content = std::fs::read_to_string(path)?;
            serde_yaml::from_str(&content)?
        } else {
            tracing::info!("No config file at {}, using env vars / defaults", path);
            AgentConfig {
                agent: AgentSection {
                    id: "auto".to_string(),
                },
                gateway: GatewaySection {
                    url: Some("ws://localhost:4443/ws".to_string()),
                    urls: Vec::new(),
                    failover_strategy: default_failover_strategy(),
                    primary_retry_secs: default_primary_retry(),
                    reconnect_interval_secs: default_reconnect_interval(),
                    tls_insecure: false,
                },
                tls: None,
                labels: std::collections::HashMap::new(),
                log_level: default_log_level(),
                mode: default_mode(),
                data_dir: None,
            }
        };

        // Env var overrides
        if let Ok(v) = std::env::var("AGENT_ID") {
            config.agent.id = v;
        }
        if let Ok(v) = std::env::var("GATEWAY_URL") {
            config.gateway.url = Some(v);
        }
        if let Ok(v) = std::env::var("GATEWAY_URLS") {
            config.gateway.urls = v.split(',').map(|s| s.trim().to_string()).collect();
        }
        if let Ok(v) = std::env::var("GATEWAY_RECONNECT_SECS") {
            if let Ok(s) = v.parse() {
                config.gateway.reconnect_interval_secs = s;
            }
        }
        if let Ok(v) = std::env::var("AGENT_MODE") {
            if v == "advisory" || v == "active" {
                config.mode = v;
            }
        }
        if let Ok(v) = std::env::var("DATA_DIR") {
            config.data_dir = Some(v);
        }
        // TLS env var overrides
        let tls_enabled = std::env::var("TLS_ENABLED")
            .ok()
            .map(|v| v == "true" || v == "1");
        let tls_cert = std::env::var("TLS_CERT_FILE").ok();
        let tls_key = std::env::var("TLS_KEY_FILE").ok();
        let tls_ca = std::env::var("TLS_CA_FILE").ok();
        // TLS_INSECURE: skip certificate verification (for self-signed certs in dev/containers)
        if let Ok(v) = std::env::var("TLS_INSECURE") {
            if v == "true" || v == "1" {
                config.gateway.tls_insecure = true;
            }
        }
        if tls_enabled.is_some() || tls_cert.is_some() || tls_key.is_some() || tls_ca.is_some() {
            let existing = config.tls.unwrap_or(TlsSection {
                enabled: false,
                cert_file: None,
                key_file: None,
                ca_file: None,
            });
            config.tls = Some(TlsSection {
                enabled: tls_enabled.unwrap_or(existing.enabled),
                cert_file: tls_cert.or(existing.cert_file),
                key_file: tls_key.or(existing.key_file),
                ca_file: tls_ca.or(existing.ca_file),
            });
        }

        Ok(config)
    }

    pub fn agent_id(&self) -> uuid::Uuid {
        if self.agent.id == "auto" {
            // Generate a deterministic ID based on hostname
            let hostname = hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string());
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, hostname.as_bytes())
        } else {
            uuid::Uuid::parse_str(&self.agent.id).unwrap_or_else(|_| uuid::Uuid::new_v4())
        }
    }

    /// Returns the list of gateway URLs to try, in failover order.
    /// Supports both legacy single `url` and new `urls` list.
    pub fn gateway_urls(&self) -> Vec<String> {
        if !self.gateway.urls.is_empty() {
            return self.gateway.urls.clone();
        }
        if let Some(ref url) = self.gateway.url {
            return vec![url.clone()];
        }
        vec!["ws://localhost:4443/ws".to_string()]
    }

    /// Legacy accessor for backward compatibility.
    #[allow(dead_code)]
    pub fn gateway_url(&self) -> &str {
        if let Some(ref url) = self.gateway.url {
            return url;
        }
        if let Some(first) = self.gateway.urls.first() {
            return first;
        }
        "ws://localhost:4443/ws"
    }

    pub fn buffer_path(&self) -> String {
        let base = self.data_dir.clone().unwrap_or_else(default_data_dir);
        format!("{}/buffer-{}", base, self.agent_id())
    }

    /// Returns true if the agent is in advisory (observation-only) mode.
    /// In advisory mode, health checks run but start/stop/rebuild commands are refused.
    pub fn is_advisory(&self) -> bool {
        self.mode == "advisory"
    }

    /// Returns the configured log level filter string.
    pub fn log_level(&self) -> String {
        self.log_level
            .clone()
            .unwrap_or_else(|| "appcontrol_agent=debug".to_string())
    }
}

/// Returns the platform-appropriate data directory for the agent.
///
/// - Linux/macOS: `/var/lib/appcontrol`
/// - Windows: `C:\ProgramData\AppControl`
pub fn default_data_dir() -> String {
    #[cfg(unix)]
    {
        "/var/lib/appcontrol".to_string()
    }
    #[cfg(windows)]
    {
        std::env::var("PROGRAMDATA")
            .map(|p| format!("{}\\AppControl", p))
            .unwrap_or_else(|_| "C:\\ProgramData\\AppControl".to_string())
    }
}

/// Returns the platform-appropriate default config directory.
///
/// - Linux/macOS: `/etc/appcontrol`
/// - Windows: `C:\ProgramData\AppControl\config`
pub fn default_config_dir() -> String {
    #[cfg(unix)]
    {
        "/etc/appcontrol".to_string()
    }
    #[cfg(windows)]
    {
        std::env::var("PROGRAMDATA")
            .map(|p| format!("{}\\AppControl\\config", p))
            .unwrap_or_else(|_| "C:\\ProgramData\\AppControl\\config".to_string())
    }
}

/// Returns the default config file path for the platform.
pub fn default_config_path() -> String {
    #[cfg(unix)]
    {
        "/etc/appcontrol/agent.yaml".to_string()
    }
    #[cfg(windows)]
    {
        format!("{}\\agent.yaml", default_config_dir())
    }
}

mod hostname {
    use std::ffi::OsString;

    pub fn get() -> Result<OsString, std::io::Error> {
        Ok(OsString::from(crate::platform::gethostname()))
    }
}
