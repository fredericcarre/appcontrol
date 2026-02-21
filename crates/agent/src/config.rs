use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct AgentConfig {
    pub agent: AgentSection,
    pub gateway: GatewaySection,
    pub tls: Option<TlsSection>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentSection {
    pub id: String, // "auto" or UUID
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct GatewaySection {
    pub url: String,
    #[serde(default = "default_reconnect_interval")]
    pub reconnect_interval_secs: u64,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct TlsSection {
    pub enabled: bool,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    pub ca_file: Option<String>,
}

fn default_reconnect_interval() -> u64 {
    10
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
                    url: "ws://localhost:4443/ws".to_string(),
                    reconnect_interval_secs: default_reconnect_interval(),
                },
                tls: None,
                labels: std::collections::HashMap::new(),
            }
        };

        // Env var overrides
        if let Ok(v) = std::env::var("AGENT_ID") {
            config.agent.id = v;
        }
        if let Ok(v) = std::env::var("GATEWAY_URL") {
            config.gateway.url = v;
        }
        if let Ok(v) = std::env::var("GATEWAY_RECONNECT_SECS") {
            if let Ok(s) = v.parse() {
                config.gateway.reconnect_interval_secs = s;
            }
        }
        // TLS env var overrides
        let tls_enabled = std::env::var("TLS_ENABLED")
            .ok()
            .map(|v| v == "true" || v == "1");
        let tls_cert = std::env::var("TLS_CERT_FILE").ok();
        let tls_key = std::env::var("TLS_KEY_FILE").ok();
        let tls_ca = std::env::var("TLS_CA_FILE").ok();
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

    pub fn gateway_url(&self) -> &str {
        &self.gateway.url
    }

    pub fn buffer_path(&self) -> String {
        format!("/var/lib/appcontrol/buffer-{}", self.agent_id())
    }
}

mod hostname {
    use std::ffi::OsString;

    pub fn get() -> Result<OsString, std::io::Error> {
        Ok(OsString::from(crate::platform::gethostname()))
    }
}
