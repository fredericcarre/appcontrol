use appcontrol_common::AgentMessage;

/// Default cap for the on-disk offline buffer when `BUFFER_MAX_BYTES`
/// is unset or unparseable. 100 MB matches the upstream guidance in
/// `crates/agent/CLAUDE.md` and the public documentation in
/// `docs/HIGH_AVAILABILITY.md`.
pub const DEFAULT_BUFFER_MAX_BYTES: u64 = 100 * 1024 * 1024;

/// Resolve the per-agent buffer size cap. Reads `BUFFER_MAX_BYTES` from
/// the environment when present; falls back to `DEFAULT_BUFFER_MAX_BYTES`.
/// A value of `0` is treated as "use default" (we never disable the cap).
pub fn resolve_buffer_max_bytes() -> u64 {
    std::env::var("BUFFER_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_BUFFER_MAX_BYTES)
}

/// Offline buffer using sled embedded database.
/// Stores messages when disconnected, replays on reconnect.
/// FIFO eviction when buffer exceeds the configured cap
/// (default: `DEFAULT_BUFFER_MAX_BYTES`, override with `BUFFER_MAX_BYTES`).
#[derive(Clone)]
pub struct OfflineBuffer {
    db: sled::Db,
    max_bytes: u64,
}

impl OfflineBuffer {
    pub fn new(path: &str) -> anyhow::Result<Self> {
        Self::with_max_bytes(path, resolve_buffer_max_bytes())
    }

    /// Open the buffer with an explicit byte cap. Mostly used by tests
    /// so they don't need to mutate the environment.
    pub fn with_max_bytes(path: &str, max_bytes: u64) -> anyhow::Result<Self> {
        let db = sled::open(path)?;
        let cap = if max_bytes == 0 {
            DEFAULT_BUFFER_MAX_BYTES
        } else {
            max_bytes
        };
        tracing::info!(
            path,
            max_bytes = cap,
            "Opened agent offline buffer (FIFO eviction)"
        );
        Ok(Self { db, max_bytes: cap })
    }

    /// Return the configured cap (in bytes). Useful for diagnostics and
    /// tests; not currently called from runtime code.
    #[allow(dead_code)]
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// Store a message in the buffer (for offline mode).
    #[allow(dead_code)]
    pub fn push(&self, msg: &AgentMessage) -> anyhow::Result<()> {
        let key = chrono::Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or(0)
            .to_be_bytes();
        let value = serde_json::to_vec(msg)?;

        // Check size and evict if needed. We loop so a backlog of small
        // entries past the cap is drained, not just trimmed by one.
        while self.db.size_on_disk()? > self.max_bytes {
            match self.db.iter().next() {
                Some(Ok((oldest_key, _))) => {
                    self.db.remove(oldest_key)?;
                }
                _ => break,
            }
        }

        self.db.insert(key, value)?;
        Ok(())
    }

    /// Drain all buffered messages in chronological order.
    pub fn drain(&self) -> anyhow::Result<Vec<AgentMessage>> {
        let mut messages = Vec::new();
        let mut keys_to_remove = Vec::new();

        for entry in self.db.iter() {
            let (key, value) = entry?;
            if let Ok(msg) = serde_json::from_slice::<AgentMessage>(&value) {
                messages.push(msg);
            }
            keys_to_remove.push(key);
        }

        // Remove all drained entries
        for key in keys_to_remove {
            self.db.remove(key)?;
        }

        self.db.flush()?;
        Ok(messages)
    }

    /// Get the number of buffered messages.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.db.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.db.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_buffer_push_and_drain() {
        let dir = tempfile::tempdir().unwrap();
        let buffer = OfflineBuffer::new(dir.path().to_str().unwrap()).unwrap();

        let msg = AgentMessage::Heartbeat {
            agent_id: Uuid::new_v4(),
            cpu: 50.0,
            memory: 60.0,
            disk: None,
            at: chrono::Utc::now(),
        };

        buffer.push(&msg).unwrap();
        assert_eq!(buffer.len(), 1);

        let messages = buffer.drain().unwrap();
        assert_eq!(messages.len(), 1);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_default_max_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let buffer = OfflineBuffer::new(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(buffer.max_bytes(), DEFAULT_BUFFER_MAX_BYTES);
    }

    #[test]
    fn test_explicit_max_bytes_is_honoured() {
        let dir = tempfile::tempdir().unwrap();
        let buffer = OfflineBuffer::with_max_bytes(dir.path().to_str().unwrap(), 64 * 1024).unwrap();
        assert_eq!(buffer.max_bytes(), 64 * 1024);
    }

    #[test]
    fn test_zero_max_bytes_falls_back_to_default() {
        let dir = tempfile::tempdir().unwrap();
        let buffer = OfflineBuffer::with_max_bytes(dir.path().to_str().unwrap(), 0).unwrap();
        assert_eq!(buffer.max_bytes(), DEFAULT_BUFFER_MAX_BYTES);
    }

    #[test]
    fn test_resolve_buffer_max_bytes_reads_env_var() {
        // Save and restore so we don't pollute the test binary's env.
        let prev = std::env::var("BUFFER_MAX_BYTES").ok();
        std::env::set_var("BUFFER_MAX_BYTES", "12345");
        assert_eq!(resolve_buffer_max_bytes(), 12345);
        std::env::remove_var("BUFFER_MAX_BYTES");
        assert_eq!(resolve_buffer_max_bytes(), DEFAULT_BUFFER_MAX_BYTES);
        if let Some(v) = prev {
            std::env::set_var("BUFFER_MAX_BYTES", v);
        }
    }

    #[test]
    fn test_buffer_fifo_order() {
        let dir = tempfile::tempdir().unwrap();
        let buffer = OfflineBuffer::new(dir.path().to_str().unwrap()).unwrap();

        for i in 0..5 {
            let msg = AgentMessage::Heartbeat {
                agent_id: Uuid::new_v4(),
                cpu: i as f32,
                memory: 0.0,
                disk: None,
                at: chrono::Utc::now(),
            };
            buffer.push(&msg).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        let messages = buffer.drain().unwrap();
        assert_eq!(messages.len(), 5);

        // Verify chronological order
        for (i, msg) in messages.iter().enumerate() {
            if let AgentMessage::Heartbeat { cpu, .. } = msg {
                assert!((cpu - i as f32).abs() < f32::EPSILON);
            }
        }
    }
}
