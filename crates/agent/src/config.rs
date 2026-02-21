use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
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
pub struct GatewaySection {
    pub url: String,
    #[serde(default = "default_reconnect_interval")]
    pub reconnect_interval_secs: u64,
}

#[derive(Debug, Deserialize, Clone)]
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
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: AgentConfig = serde_yaml::from_str(&content)?;
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

// Add hostname crate dependency alternative - use nix for hostname
mod hostname {
    use std::ffi::OsString;

    pub fn get() -> Result<OsString, std::io::Error> {
        let mut buf = [0u8; 256];
        let result = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Ok(OsString::from(
            String::from_utf8_lossy(&buf[..len]).to_string(),
        ))
    }
}
