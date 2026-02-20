use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;
use sha2::{Sha256, Digest};

use appcontrol_common::{AgentMessage, CheckResult, CheckType, ComponentConfig};

/// Manages periodic health check scheduling for all assigned components.
pub struct CheckScheduler {
    agent_id: Uuid,
    msg_tx: mpsc::UnboundedSender<AgentMessage>,
    components: tokio::sync::RwLock<HashMap<Uuid, ComponentConfig>>,
    /// Deduplication: SHA-256 of command → last result + timestamp
    dedup_cache: tokio::sync::RwLock<HashMap<String, (i32, chrono::DateTime<chrono::Utc>)>>,
}

impl CheckScheduler {
    pub fn new(agent_id: Uuid, msg_tx: mpsc::UnboundedSender<AgentMessage>) -> Self {
        Self {
            agent_id,
            msg_tx,
            components: tokio::sync::RwLock::new(HashMap::new()),
            dedup_cache: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    pub async fn update_components(&self, configs: Vec<ComponentConfig>) {
        let mut components = self.components.write().await;
        components.clear();
        for config in configs {
            components.insert(config.component_id, config);
        }
    }

    pub async fn run(self) {
        let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(60));

        loop {
            heartbeat_interval.tick().await;

            // Send heartbeat
            let mut sys = sysinfo::System::new();
            sys.refresh_memory();
            sys.refresh_cpu_usage();
            let cpu = sysinfo::System::load_average().one as f32;
            let memory = if sys.total_memory() > 0 {
                sys.used_memory() as f32 / sys.total_memory() as f32 * 100.0
            } else {
                0.0
            };

            let _ = self.msg_tx.send(AgentMessage::Heartbeat {
                agent_id: self.agent_id,
                cpu,
                memory,
                at: chrono::Utc::now(),
            });

            // Run health checks for all components
            self.run_health_checks().await;
        }
    }

    async fn run_health_checks(&self) {
        let components = self.components.read().await;
        let mut executed_cmds: HashMap<String, (i32, String)> = HashMap::new();

        for (comp_id, config) in components.iter() {
            if let Some(ref check_cmd) = config.check_cmd {
                // Deduplication: hash the command
                let cmd_hash = hash_command(check_cmd);

                let (exit_code, stdout) = if let Some(cached) = executed_cmds.get(&cmd_hash) {
                    cached.clone()
                } else {
                    // Execute the check
                    let timeout = Duration::from_secs(30);
                    match crate::executor::execute_sync(check_cmd, timeout).await {
                        Ok(result) => {
                            let entry = (result.exit_code, result.stdout.clone());
                            executed_cmds.insert(cmd_hash, entry.clone());
                            entry
                        }
                        Err(_) => (-1, String::new()),
                    }
                };

                // Check if this is a delta (exit code changed from last known)
                let should_send = {
                    let cache = self.dedup_cache.read().await;
                    match cache.get(&comp_id.to_string()) {
                        Some((last_code, _)) => *last_code != exit_code,
                        None => true, // First check, always send
                    }
                };

                if should_send {
                    // Update cache
                    {
                        let mut cache = self.dedup_cache.write().await;
                        cache.insert(comp_id.to_string(), (exit_code, chrono::Utc::now()));
                    }

                    let _ = self.msg_tx.send(AgentMessage::CheckResult(CheckResult {
                        component_id: *comp_id,
                        check_type: CheckType::Health,
                        exit_code,
                        stdout: Some(stdout),
                        duration_ms: 0,
                        at: chrono::Utc::now(),
                    }));
                }
            }
        }
    }
}

fn hash_command(cmd: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(cmd.as_bytes());
    format!("{:x}", hasher.finalize())
}
