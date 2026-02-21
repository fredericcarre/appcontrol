use appcontrol_common::AgentMessage;

#[allow(dead_code)]
const MAX_BUFFER_SIZE: u64 = 100 * 1024 * 1024; // 100MB

/// Offline buffer using sled embedded database.
/// Stores messages when disconnected, replays on reconnect.
/// FIFO eviction when buffer exceeds 100MB.
#[derive(Clone)]
pub struct OfflineBuffer {
    db: sled::Db,
}

impl OfflineBuffer {
    pub fn new(path: &str) -> anyhow::Result<Self> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }

    /// Store a message in the buffer (for offline mode).
    #[allow(dead_code)]
    pub fn push(&self, msg: &AgentMessage) -> anyhow::Result<()> {
        let key = chrono::Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or(0)
            .to_be_bytes();
        let value = serde_json::to_vec(msg)?;

        // Check size and evict if needed
        if self.db.size_on_disk()? > MAX_BUFFER_SIZE {
            // Remove oldest entries (FIFO)
            if let Some(Ok((oldest_key, _))) = self.db.iter().next() {
                self.db.remove(oldest_key)?;
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
            at: chrono::Utc::now(),
        };

        buffer.push(&msg).unwrap();
        assert_eq!(buffer.len(), 1);

        let messages = buffer.drain().unwrap();
        assert_eq!(messages.len(), 1);
        assert!(buffer.is_empty());
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
                at: chrono::Utc::now(),
            };
            buffer.push(&msg).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1));
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
