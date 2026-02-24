use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("Download failed: {0}")]
    Download(String),
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to determine current executable path")]
    NoExePath,
}

/// Download, verify, and atomically replace the agent binary, then restart.
///
/// Steps:
/// 1. Download the new binary to a temp file alongside the current binary
/// 2. Compute SHA-256 of the downloaded file and verify against expected checksum
/// 3. Atomic rename: move current binary to .old, move new binary to current path
/// 4. Re-exec the process (Unix) or schedule restart (Windows)
pub async fn perform_update(
    binary_url: &str,
    checksum_sha256: &str,
    target_version: &str,
) -> Result<(), UpdateError> {
    let current_exe = std::env::current_exe().map_err(|_| UpdateError::NoExePath)?;
    let parent_dir = current_exe.parent().ok_or(UpdateError::NoExePath)?;

    tracing::info!(
        version = target_version,
        url = binary_url,
        "Starting agent self-update"
    );

    // Step 1: Download to a temp file in the same directory (ensures same filesystem for atomic rename)
    let temp_path = parent_dir.join(format!(".appcontrol-agent-update-{}", target_version));
    download_binary(binary_url, &temp_path).await?;

    // Step 2: Verify checksum
    verify_checksum(&temp_path, checksum_sha256).await?;

    // Step 3: Set executable permission (Unix)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&temp_path, perms)?;
    }

    // Step 4: Atomic swap
    let backup_path = parent_dir.join(".appcontrol-agent.old");
    atomic_replace(&current_exe, &temp_path, &backup_path)?;

    tracing::info!(
        version = target_version,
        "Binary replaced successfully — restarting agent"
    );

    // Step 5: Restart
    restart_agent(&current_exe)?;

    Ok(())
}

/// Download a binary from a URL to a local file path.
async fn download_binary(url: &str, dest: &PathBuf) -> Result<(), UpdateError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| UpdateError::Download(e.to_string()))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| UpdateError::Download(e.to_string()))?;

    if !response.status().is_success() {
        return Err(UpdateError::Download(format!(
            "HTTP {} from {}",
            response.status(),
            url
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpdateError::Download(e.to_string()))?;

    tokio::fs::write(dest, &bytes).await?;

    tracing::info!(
        size_bytes = bytes.len(),
        path = %dest.display(),
        "Downloaded update binary"
    );

    Ok(())
}

/// Verify SHA-256 checksum of a file.
async fn verify_checksum(path: &PathBuf, expected: &str) -> Result<(), UpdateError> {
    let data = tokio::fs::read(path).await?;

    let mut hasher = Sha256::new();
    hasher.update(&data);
    let actual = hex::encode(hasher.finalize());

    if actual != expected.to_lowercase() {
        // Clean up the bad download
        let _ = tokio::fs::remove_file(path).await;
        return Err(UpdateError::ChecksumMismatch {
            expected: expected.to_string(),
            actual,
        });
    }

    tracing::info!(sha256 = %actual, "Checksum verified");
    Ok(())
}

/// Atomic binary replacement: current → .old, new → current.
fn atomic_replace(
    current: &std::path::Path,
    new_binary: &std::path::Path,
    backup: &std::path::Path,
) -> Result<(), UpdateError> {
    // Remove old backup if it exists
    if backup.exists() {
        std::fs::remove_file(backup)?;
    }

    // Rename current → backup
    std::fs::rename(current, backup)?;

    // Rename new → current
    match std::fs::rename(new_binary, current) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Rollback: restore backup
            tracing::error!("Failed to place new binary, rolling back: {}", e);
            let _ = std::fs::rename(backup, current);
            Err(e.into())
        }
    }
}

/// Restart the agent process.
#[cfg(unix)]
fn restart_agent(exe_path: &std::path::Path) -> Result<(), UpdateError> {
    use std::os::unix::process::CommandExt;

    let args: Vec<String> = std::env::args().collect();

    tracing::info!(
        "Re-executing agent: {} {:?}",
        exe_path.display(),
        &args[1..]
    );

    // exec() replaces the current process image — this never returns on success
    let err = std::process::Command::new(exe_path).args(&args[1..]).exec();

    // If we reach here, exec() failed
    Err(UpdateError::Io(err))
}

#[cfg(windows)]
fn restart_agent(exe_path: &std::path::Path) -> Result<(), UpdateError> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    tracing::info!(
        "Spawning new agent process and exiting: {}",
        exe_path.display()
    );

    std::process::Command::new(exe_path)
        .args(&args)
        .spawn()
        .map_err(|e| UpdateError::Io(e))?;

    // Exit the current process — the new one will take over
    std::process::exit(0);
}

// ---------------------------------------------------------------------------
// Air-gap update: receive binary chunks via WebSocket, assemble, verify, apply
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Mutex;

use appcontrol_common::{AgentMessage, UpdateStatus};

/// In-progress chunked downloads, keyed by update_id.
static CHUNK_BUFFERS: std::sync::LazyLock<Mutex<HashMap<uuid::Uuid, ChunkBuffer>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

struct ChunkBuffer {
    target_version: String,
    checksum_sha256: String,
    total_chunks: u32,
    total_size: u64,
    chunks: HashMap<u32, Vec<u8>>,
}

/// Handle an incoming binary chunk for air-gap update.
/// Accumulates chunks in memory, and when all chunks are received,
/// assembles the binary, verifies checksum, and applies the update.
#[allow(clippy::too_many_arguments)]
pub async fn handle_binary_chunk(
    update_id: uuid::Uuid,
    target_version: &str,
    checksum_sha256: &str,
    chunk_index: u32,
    total_chunks: u32,
    total_size: u64,
    data_base64: &str,
    agent_id: uuid::Uuid,
    msg_tx: &tokio::sync::mpsc::UnboundedSender<AgentMessage>,
) {
    // Decode base64 chunk
    let chunk_data = match base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        data_base64,
    ) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Failed to decode base64 chunk: {}", e);
            let _ = msg_tx.send(AgentMessage::UpdateProgress {
                update_id,
                agent_id,
                chunks_received: 0,
                status: UpdateStatus::Failed,
                error: Some(format!("Base64 decode error: {}", e)),
            });
            return;
        }
    };

    let all_received = {
        let mut buffers = CHUNK_BUFFERS.lock().unwrap();
        let buf = buffers.entry(update_id).or_insert_with(|| ChunkBuffer {
            target_version: target_version.to_string(),
            checksum_sha256: checksum_sha256.to_string(),
            total_chunks,
            total_size,
            chunks: HashMap::new(),
        });
        buf.chunks.insert(chunk_index, chunk_data);
        let received = buf.chunks.len() as u32;

        let _ = msg_tx.send(AgentMessage::UpdateProgress {
            update_id,
            agent_id,
            chunks_received: received,
            status: UpdateStatus::Downloading,
            error: None,
        });

        received == total_chunks
    };

    if all_received {
        // Assemble and apply
        let (version, checksum, assembled) = {
            let mut buffers = CHUNK_BUFFERS.lock().unwrap();
            if let Some(buf) = buffers.remove(&update_id) {
                let mut binary = Vec::with_capacity(buf.total_size as usize);
                for i in 0..buf.total_chunks {
                    if let Some(chunk) = buf.chunks.get(&i) {
                        binary.extend_from_slice(chunk);
                    }
                }
                (buf.target_version, buf.checksum_sha256, binary)
            } else {
                return;
            }
        };

        let _ = msg_tx.send(AgentMessage::UpdateProgress {
            update_id,
            agent_id,
            chunks_received: total_chunks,
            status: UpdateStatus::Verifying,
            error: None,
        });

        // Write to temp file and verify
        let current_exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(_) => {
                let _ = msg_tx.send(AgentMessage::UpdateProgress {
                    update_id,
                    agent_id,
                    chunks_received: total_chunks,
                    status: UpdateStatus::Failed,
                    error: Some("Cannot determine current exe path".to_string()),
                });
                return;
            }
        };
        let parent_dir = current_exe.parent().unwrap();
        let temp_path = parent_dir.join(format!(".appcontrol-agent-update-{}", version));

        if let Err(e) = tokio::fs::write(&temp_path, &assembled).await {
            let _ = msg_tx.send(AgentMessage::UpdateProgress {
                update_id,
                agent_id,
                chunks_received: total_chunks,
                status: UpdateStatus::Failed,
                error: Some(format!("Write error: {}", e)),
            });
            return;
        }

        if let Err(e) = verify_checksum(&temp_path, &checksum).await {
            let _ = msg_tx.send(AgentMessage::UpdateProgress {
                update_id,
                agent_id,
                chunks_received: total_chunks,
                status: UpdateStatus::Failed,
                error: Some(format!("Checksum mismatch: {}", e)),
            });
            return;
        }

        let _ = msg_tx.send(AgentMessage::UpdateProgress {
            update_id,
            agent_id,
            chunks_received: total_chunks,
            status: UpdateStatus::Applying,
            error: None,
        });

        // Set executable permission on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o755));
        }

        // Atomic replace and restart
        let backup_path = parent_dir.join(".appcontrol-agent.old");
        if let Err(e) = atomic_replace(&current_exe, &temp_path, &backup_path) {
            let _ = msg_tx.send(AgentMessage::UpdateProgress {
                update_id,
                agent_id,
                chunks_received: total_chunks,
                status: UpdateStatus::Failed,
                error: Some(format!("Atomic replace failed: {}", e)),
            });
            return;
        }

        let _ = msg_tx.send(AgentMessage::UpdateProgress {
            update_id,
            agent_id,
            chunks_received: total_chunks,
            status: UpdateStatus::Complete,
            error: None,
        });

        tracing::info!(
            "Air-gap update to v{} complete — restarting agent",
            version
        );
        let _ = restart_agent(&current_exe);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_verify_checksum_matches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_binary");
        tokio::fs::write(&path, b"hello world").await.unwrap();

        // SHA-256 of "hello world"
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert!(verify_checksum(&path, expected).await.is_ok());
    }

    #[tokio::test]
    async fn test_verify_checksum_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_binary");
        tokio::fs::write(&path, b"hello world").await.unwrap();

        let wrong = "0000000000000000000000000000000000000000000000000000000000000000";
        let result = verify_checksum(&path, wrong).await;
        assert!(matches!(result, Err(UpdateError::ChecksumMismatch { .. })));
    }

    #[test]
    fn test_atomic_replace() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("agent");
        let new_bin = dir.path().join("agent.new");
        let backup = dir.path().join("agent.old");

        std::fs::write(&current, b"old binary").unwrap();
        std::fs::write(&new_bin, b"new binary").unwrap();

        atomic_replace(&current, &new_bin, &backup).unwrap();

        assert_eq!(std::fs::read(&current).unwrap(), b"new binary");
        assert_eq!(std::fs::read(&backup).unwrap(), b"old binary");
        assert!(!new_bin.exists());
    }

    #[test]
    fn test_atomic_replace_rollback_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("agent");
        let new_bin = dir.path().join("nonexistent"); // doesn't exist — rename will fail
        let backup = dir.path().join("agent.old");

        std::fs::write(&current, b"old binary").unwrap();

        // This should fail because new_bin doesn't exist, but backup will be restored
        let result = atomic_replace(&current, &new_bin, &backup);
        assert!(result.is_err());
        // current should be restored from backup
        assert_eq!(std::fs::read(&current).unwrap(), b"old binary");
    }
}
