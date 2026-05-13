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

/// Offline buffer using redb embedded database.
/// Stores messages when disconnected, replays on reconnect.
/// FIFO eviction once the row count approximates the configured byte cap
/// (default: `DEFAULT_BUFFER_MAX_BYTES`, override with `BUFFER_MAX_BYTES`).
///
/// redb does not expose a cheap on-disk size metric the way sled did,
/// so we translate the byte cap into a row cap via `AVG_ENTRY_BYTES`. The
/// worst case is the file grows a bit beyond the configured cap before
/// eviction kicks in, never unbounded growth.
#[derive(Clone)]
pub struct OfflineBuffer {
    db: Arc<Database>,
    max_bytes: u64,
}

/// Conservative average entry size used to convert `max_bytes` into a row
/// cap. Real entries can be larger; choose a value that errs toward
/// evicting earlier rather than later.
const AVG_ENTRY_BYTES: u64 = 1024;

impl OfflineBuffer {
    pub fn new(path: &str) -> anyhow::Result<Self> {
        Self::with_max_bytes(path, resolve_buffer_max_bytes())
    }

    /// Open the buffer with an explicit byte cap. Mostly used by tests
    /// so they don't need to mutate the environment.
    pub fn with_max_bytes(path: &str, max_bytes: u64) -> anyhow::Result<Self> {
        let cap = if max_bytes == 0 {
            DEFAULT_BUFFER_MAX_BYTES
        } else {
            max_bytes
        };

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
        tracing::info!(
            path = %resolved.display(),
            max_bytes = cap,
            "Opened agent offline buffer (FIFO eviction)"
        );
        Ok(Self {
            db: Arc::new(db),
            max_bytes: cap,
        })
    }

    /// Return the configured byte cap. Useful for diagnostics and tests;
    /// not currently called from runtime code.
    #[allow(dead_code)]
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// Soft row cap derived from the configured byte cap. Internal.
    fn soft_row_cap(&self) -> u64 {
        (self.max_bytes / AVG_ENTRY_BYTES).max(1)
    }

    /// Store a message in the buffer (for offline mode).
    /// When over the soft row cap, removes the oldest entry first (FIFO).
    #[allow(dead_code)]
    pub fn push(&self, msg: &AgentMessage) -> anyhow::Result<()> {
        let key: i128 = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as i128;
        let value = serde_json::to_vec(msg)?;

        // FIFO eviction: redb does not expose a cheap file-size metric
        // the way sled did, so we approximate the configured byte cap
        // via a row count derived from `AVG_ENTRY_BYTES`. The worst case
        // is the file grows a bit beyond the cap before eviction kicks
        // in, not unbounded growth.
        let soft_row_cap = self.soft_row_cap();
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE)?;
            if table.len()? >= soft_row_cap {
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
