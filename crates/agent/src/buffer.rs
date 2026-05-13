use appcontrol_common::AgentMessage;
use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use std::path::Path;
use std::sync::Arc;

// Migrated from sled to redb (October 2025).
// sled 0.34 was the last release and pulled `fxhash` + `instant`,
// both unmaintained (RUSTSEC-2025-0057, RUSTSEC-2024-0384). redb is
// a pure-Rust, ACID, maintained embedded KV store with a similar
// ordered-by-key contract.
//
// Storage layout
//   - one redb database file (or directory containing it)
//   - one table named "messages"
//   - key   : i128 big-endian (timestamp in nanoseconds, FIFO order)
//             — i128 fits in a single redb fixed-width key, and we
//               keep the original nanosecond precision and BE ordering
//               so chronological iteration reproduces sled's order.
//   - value : raw bytes (serde_json of AgentMessage)

const TABLE: TableDefinition<i128, &[u8]> = TableDefinition::new("messages");

#[allow(dead_code)]
const MAX_BUFFER_SIZE: u64 = 100 * 1024 * 1024; // 100MB

/// Offline buffer using redb embedded database.
/// Stores messages when disconnected, replays on reconnect.
/// FIFO eviction when buffer exceeds ~100MB on disk.
#[derive(Clone)]
pub struct OfflineBuffer {
    db: Arc<Database>,
}

impl OfflineBuffer {
    pub fn new(path: &str) -> anyhow::Result<Self> {
        // redb opens a file, not a directory (unlike sled). If the
        // caller passed a directory path (typical for the legacy
        // sled layout) we create a `buffer.redb` file inside it; if
        // they passed a file path directly we use it as-is.
        let resolved = {
            let p = Path::new(path);
            if p.is_dir() {
                p.join("buffer.redb")
            } else if p.extension().is_none() && !p.exists() {
                // Legacy sled callers passed a non-existent dir-like
                // path. Honour the same intent by creating it as a
                // directory + file inside.
                std::fs::create_dir_all(p)?;
                p.join("buffer.redb")
            } else {
                p.to_path_buf()
            }
        };
        let db = Database::create(&resolved).map_err(|e| {
            anyhow::anyhow!(
                "Failed to open redb buffer at {}: {}",
                resolved.display(),
                e
            )
        })?;
        // Ensure the table exists so empty databases behave like sled
        // (no error on first read).
        {
            let write_txn = db.begin_write()?;
            write_txn.open_table(TABLE)?;
            write_txn.commit()?;
        }
        Ok(Self { db: Arc::new(db) })
    }

    /// Store a message in the buffer (for offline mode).
    /// When over the soft row cap, removes the oldest entry first (FIFO).
    #[allow(dead_code)]
    pub fn push(&self, msg: &AgentMessage) -> anyhow::Result<()> {
        let key: i128 = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as i128;
        let value = serde_json::to_vec(msg)?;

        // FIFO eviction: redb does not expose a cheap file-size metric
        // the way sled did (`size_on_disk()`), so we approximate the
        // 100 MB cap via a row count: 100 000 entries × ~1 KB average
        // payload. Conservative — real entries can be larger, but the
        // worst case is the file grows a bit beyond 100 MB before the
        // cap kicks in, not unbounded growth.
        const SOFT_ROW_CAP: u64 = 100_000;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE)?;
            if table.len()? >= SOFT_ROW_CAP {
                // Compute the oldest key in a separate scope so the
                // iterator's immutable borrow of `table` is released
                // before we mutably borrow it for `remove()`.
                let oldest_key: Option<i128> = {
                    let mut iter = table.iter()?;
                    match iter.next() {
                        Some(Ok(entry)) => Some(entry.0.value()),
                        _ => None,
                    }
                };
                if let Some(k) = oldest_key {
                    table.remove(k)?;
                }
            }
            table.insert(key, value.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Drain all buffered messages in chronological order.
    pub fn drain(&self) -> anyhow::Result<Vec<AgentMessage>> {
        let mut messages = Vec::new();
        let mut keys_to_remove: Vec<i128> = Vec::new();

        // Read pass — collect everything, in key-sorted (== chrono) order.
        {
            let read_txn = self.db.begin_read()?;
            let table = read_txn.open_table(TABLE)?;
            for entry in table.iter()? {
                let entry = entry?;
                let key: i128 = entry.0.value();
                let value_bytes: &[u8] = entry.1.value();
                if let Ok(msg) = serde_json::from_slice::<AgentMessage>(value_bytes) {
                    messages.push(msg);
                }
                keys_to_remove.push(key);
            }
        }

        // Write pass — atomically clear everything we just collected.
        if !keys_to_remove.is_empty() {
            let write_txn = self.db.begin_write()?;
            {
                let mut table = write_txn.open_table(TABLE)?;
                for key in &keys_to_remove {
                    table.remove(*key)?;
                }
            }
            write_txn.commit()?;
        }

        Ok(messages)
    }

    /// Get the number of buffered messages.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        let Ok(read_txn) = self.db.begin_read() else {
            return 0;
        };
        let Ok(table) = read_txn.open_table(TABLE) else {
            return 0;
        };
        table.len().unwrap_or(0) as usize
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
