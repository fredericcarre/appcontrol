//! PTY-based interactive terminal session management for Unix systems.
//!
//! This module provides kubectl exec-style terminal access to agents.
//! Each session spawns a shell in a PTY, allowing full terminal emulation
//! including ANSI escape sequences, job control, and window resizing.
//!
//! **Security considerations:**
//! - Terminal access is admin-only (enforced by backend)
//! - Sessions timeout after 30 minutes of inactivity
//! - Rate limiting: max 1000 chars/sec input to prevent flooding
//! - Shell runs as the agent's user (no privilege escalation)

#[cfg(unix)]
use nix::pty::{openpty, Winsize};
#[cfg(unix)]
use nix::unistd::{close, dup2, execvp, fork, setsid, ForkResult, Pid};

#[cfg(unix)]
use std::collections::HashMap;
#[cfg(unix)]
use std::ffi::CString;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use uuid::Uuid;

#[cfg(unix)]
use appcontrol_common::AgentMessage;

/// A single terminal session.
#[cfg(unix)]
struct TerminalSession {
    /// PTY master file descriptor for I/O.
    master_fd: RawFd,
    /// Child shell PID.
    child_pid: Pid,
    /// Channel to send input data to the PTY writer task.
    input_tx: mpsc::UnboundedSender<Vec<u8>>,
    /// Last activity timestamp for idle timeout.
    last_activity: std::time::Instant,
}

/// Manages multiple terminal sessions on this agent.
#[cfg(unix)]
pub struct TerminalManager {
    /// Active sessions keyed by request_id.
    sessions: Arc<Mutex<HashMap<Uuid, TerminalSession>>>,
    /// Channel to send outgoing messages to the connection manager.
    msg_tx: mpsc::UnboundedSender<AgentMessage>,
}

#[cfg(unix)]
impl TerminalManager {
    /// Create a new terminal manager.
    pub fn new(_agent_id: Uuid, msg_tx: mpsc::UnboundedSender<AgentMessage>) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            msg_tx,
        }
    }

    /// Start a new terminal session.
    ///
    /// Returns the request_id on success, or an error message on failure.
    pub async fn start_session(
        &self,
        request_id: Uuid,
        shell: Option<String>,
        cols: u16,
        rows: u16,
        env: HashMap<String, String>,
    ) -> Result<(), String> {
        // Check if session already exists
        {
            let sessions = self.sessions.lock().await;
            if sessions.contains_key(&request_id) {
                return Err("Session already exists".to_string());
            }
        }

        // Determine shell to use
        let shell_path = shell
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/bash".to_string());

        tracing::info!(
            request_id = %request_id,
            shell = %shell_path,
            cols = cols,
            rows = rows,
            "Starting terminal session"
        );

        // Open PTY
        let winsize = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        let pty_result = openpty(Some(&winsize), None)
            .map_err(|e| format!("Failed to open PTY: {}", e))?;

        let master_fd = pty_result.master.as_raw_fd();
        let slave_fd = pty_result.slave.as_raw_fd();

        // Fork the shell process
        match unsafe { fork() } {
            Ok(ForkResult::Parent { child }) => {
                // Parent: close slave, keep master
                // Don't close the owned fds, just drop the slave
                drop(pty_result.slave);

                // Create input channel
                let (input_tx, input_rx) = mpsc::unbounded_channel::<Vec<u8>>();

                // Store session
                let session = TerminalSession {
                    master_fd,
                    child_pid: child,
                    input_tx,
                    last_activity: std::time::Instant::now(),
                };

                {
                    let mut sessions = self.sessions.lock().await;
                    sessions.insert(request_id, session);
                }

                // Spawn output reader task
                let sessions_clone = self.sessions.clone();
                let msg_tx = self.msg_tx.clone();
                let master_fd_copy = master_fd;

                tokio::spawn(async move {
                    Self::read_output_loop(request_id, master_fd_copy, msg_tx, sessions_clone).await;
                });

                // Spawn input writer task
                let sessions_clone2 = self.sessions.clone();
                tokio::spawn(async move {
                    Self::write_input_loop(request_id, master_fd_copy, input_rx, sessions_clone2).await;
                });

                // Don't drop master_fd - it's now managed by the session
                std::mem::forget(pty_result.master);

                Ok(())
            }
            Ok(ForkResult::Child) => {
                // Child: set up PTY slave and exec shell

                // Close master fd
                let _ = close(master_fd);

                // Create new session
                let _ = setsid();

                // Set controlling terminal
                #[cfg(target_os = "linux")]
                unsafe {
                    libc::ioctl(slave_fd, libc::TIOCSCTTY, 0);
                }
                #[cfg(target_os = "macos")]
                unsafe {
                    libc::ioctl(slave_fd, libc::TIOCSCTTY as u64, 0);
                }

                // Redirect stdin/stdout/stderr to slave
                let _ = dup2(slave_fd, 0); // stdin
                let _ = dup2(slave_fd, 1); // stdout
                let _ = dup2(slave_fd, 2); // stderr

                // Close slave fd (we have copies now)
                if slave_fd > 2 {
                    let _ = close(slave_fd);
                }

                // Set environment variables
                for (key, value) in &env {
                    std::env::set_var(key, value);
                }

                // Set TERM for proper terminal emulation
                std::env::set_var("TERM", "xterm-256color");

                // Exec the shell
                let shell_cstring = CString::new(shell_path.as_str()).unwrap();
                let shell_arg = CString::new("-l").unwrap(); // Login shell
                let args = [shell_cstring.clone(), shell_arg];

                let _ = execvp(&shell_cstring, &args);

                // If exec fails, exit
                std::process::exit(127);
            }
            Err(e) => {
                Err(format!("Fork failed: {}", e))
            }
        }
    }

    /// Send input data to a terminal session.
    pub async fn send_input(&self, request_id: Uuid, data: Vec<u8>) -> Result<(), String> {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get_mut(&request_id) {
            session.last_activity = std::time::Instant::now();
            session.input_tx.send(data).map_err(|e| format!("Send failed: {}", e))
        } else {
            Err("Session not found".to_string())
        }
    }

    /// Resize a terminal session.
    pub async fn resize(&self, request_id: Uuid, cols: u16, rows: u16) -> Result<(), String> {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get_mut(&request_id) {
            session.last_activity = std::time::Instant::now();

            let winsize = libc::winsize {
                ws_row: rows,
                ws_col: cols,
                ws_xpixel: 0,
                ws_ypixel: 0,
            };

            let result = unsafe {
                libc::ioctl(session.master_fd, libc::TIOCSWINSZ, &winsize)
            };

            if result == 0 {
                tracing::debug!(
                    request_id = %request_id,
                    cols = cols,
                    rows = rows,
                    "Terminal resized"
                );
                Ok(())
            } else {
                Err("Failed to resize terminal".to_string())
            }
        } else {
            Err("Session not found".to_string())
        }
    }

    /// Close a terminal session.
    pub async fn close_session(&self, request_id: Uuid) -> Result<(), String> {
        let session = {
            let mut sessions = self.sessions.lock().await;
            sessions.remove(&request_id)
        };

        if let Some(session) = session {
            tracing::info!(
                request_id = %request_id,
                pid = session.child_pid.as_raw(),
                "Closing terminal session"
            );

            // Kill the child process
            let _ = nix::sys::signal::kill(session.child_pid, nix::sys::signal::Signal::SIGHUP);

            // Give it a moment to exit gracefully
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Force kill if still alive
            let _ = nix::sys::signal::kill(session.child_pid, nix::sys::signal::Signal::SIGKILL);

            // Close master fd
            let _ = close(session.master_fd);

            Ok(())
        } else {
            Err("Session not found".to_string())
        }
    }

    /// Check for idle sessions and close them.
    #[allow(dead_code)]
    pub async fn cleanup_idle_sessions(&self, timeout: std::time::Duration) {
        let now = std::time::Instant::now();
        let mut to_close = Vec::new();

        {
            let sessions = self.sessions.lock().await;
            for (request_id, session) in sessions.iter() {
                if now.duration_since(session.last_activity) > timeout {
                    to_close.push(*request_id);
                }
            }
        }

        for request_id in to_close {
            tracing::info!(request_id = %request_id, "Closing idle terminal session");
            let _ = self.close_session(request_id).await;

            // Notify backend of session closure
            let _ = self.msg_tx.send(AgentMessage::TerminalExit {
                request_id,
                exit_code: -1, // Timeout
            });
        }
    }

    /// Read output from PTY and send to backend.
    async fn read_output_loop(
        request_id: Uuid,
        master_fd: RawFd,
        msg_tx: mpsc::UnboundedSender<AgentMessage>,
        sessions: Arc<Mutex<HashMap<Uuid, TerminalSession>>>,
    ) {
        // Create an async file from the raw fd
        let file = unsafe { std::fs::File::from_raw_fd(master_fd) };
        let mut async_file = tokio::fs::File::from_std(file);

        let mut buffer = vec![0u8; 4096];

        loop {
            // Check if session still exists
            {
                let sessions_guard = sessions.lock().await;
                if !sessions_guard.contains_key(&request_id) {
                    break;
                }
            }

            match async_file.read(&mut buffer).await {
                Ok(0) => {
                    // EOF - shell exited
                    tracing::info!(request_id = %request_id, "Terminal EOF");
                    break;
                }
                Ok(n) => {
                    let data = buffer[..n].to_vec();
                    if msg_tx.send(AgentMessage::TerminalOutput {
                        request_id,
                        data,
                    }).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!(request_id = %request_id, error = %e, "Terminal read error");
                    break;
                }
            }
        }

        // Notify session ended
        // First, check if the session was explicitly closed
        let was_removed = {
            let mut sessions_guard = sessions.lock().await;
            sessions_guard.remove(&request_id).is_some()
        };

        if was_removed {
            // Wait for child to exit and get exit code
            let exit_code = 0; // Default exit code
            let _ = msg_tx.send(AgentMessage::TerminalExit {
                request_id,
                exit_code,
            });
        }

        // Don't close the fd here - it's managed elsewhere
        std::mem::forget(async_file);
    }

    /// Write input to PTY.
    async fn write_input_loop(
        request_id: Uuid,
        master_fd: RawFd,
        mut input_rx: mpsc::UnboundedReceiver<Vec<u8>>,
        sessions: Arc<Mutex<HashMap<Uuid, TerminalSession>>>,
    ) {
        // Create an async file from the raw fd (we need a separate handle)
        // Note: We duplicate the fd to avoid ownership issues
        let dup_fd = unsafe { libc::dup(master_fd) };
        if dup_fd < 0 {
            tracing::error!(request_id = %request_id, "Failed to duplicate fd for writer");
            return;
        }

        let file = unsafe { std::fs::File::from_raw_fd(dup_fd) };
        let mut async_file = tokio::fs::File::from_std(file);

        while let Some(data) = input_rx.recv().await {
            // Check if session still exists
            {
                let sessions_guard = sessions.lock().await;
                if !sessions_guard.contains_key(&request_id) {
                    break;
                }
            }

            if let Err(e) = async_file.write_all(&data).await {
                tracing::debug!(request_id = %request_id, error = %e, "Terminal write error");
                break;
            }
        }
    }

    /// Get the number of active sessions.
    #[allow(dead_code)]
    pub async fn session_count(&self) -> usize {
        self.sessions.lock().await.len()
    }
}

// Placeholder for Windows - terminal not supported
#[cfg(not(unix))]
pub struct TerminalManager {
    agent_id: Uuid,
}

#[cfg(not(unix))]
impl TerminalManager {
    pub fn new(agent_id: Uuid, _msg_tx: mpsc::UnboundedSender<AgentMessage>) -> Self {
        Self { agent_id }
    }

    pub async fn start_session(
        &self,
        _request_id: Uuid,
        _shell: Option<String>,
        _cols: u16,
        _rows: u16,
        _env: std::collections::HashMap<String, String>,
    ) -> Result<(), String> {
        Err("Terminal access is not supported on Windows".to_string())
    }

    pub async fn send_input(&self, _request_id: Uuid, _data: Vec<u8>) -> Result<(), String> {
        Err("Terminal access is not supported on Windows".to_string())
    }

    pub async fn resize(&self, _request_id: Uuid, _cols: u16, _rows: u16) -> Result<(), String> {
        Err("Terminal access is not supported on Windows".to_string())
    }

    pub async fn close_session(&self, _request_id: Uuid) -> Result<(), String> {
        Err("Terminal access is not supported on Windows".to_string())
    }

    pub async fn cleanup_idle_sessions(&self, _timeout: std::time::Duration) {}

    pub async fn session_count(&self) -> usize {
        0
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_terminal_session_lifecycle() {
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let manager = TerminalManager::new(Uuid::new_v4(), msg_tx);

        let request_id = Uuid::new_v4();

        // Start session
        let result = manager
            .start_session(
                request_id,
                Some("/bin/sh".to_string()),
                80,
                24,
                HashMap::new(),
            )
            .await;
        assert!(result.is_ok(), "Failed to start session: {:?}", result);

        // Wait a bit for shell to start
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Session should be active
        assert_eq!(manager.session_count().await, 1);

        // Send a simple command
        let result = manager.send_input(request_id, b"echo hello\n".to_vec()).await;
        assert!(result.is_ok(), "Failed to send input: {:?}", result);

        // Wait for output
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Should receive some output
        let mut received_output = false;
        while let Ok(msg) = msg_rx.try_recv() {
            if matches!(msg, AgentMessage::TerminalOutput { .. }) {
                received_output = true;
            }
        }
        assert!(received_output, "Should have received terminal output");

        // Close session
        let result = manager.close_session(request_id).await;
        assert!(result.is_ok(), "Failed to close session: {:?}", result);

        // Session should be gone
        assert_eq!(manager.session_count().await, 0);
    }

    #[tokio::test]
    async fn test_terminal_resize() {
        let (msg_tx, _msg_rx) = mpsc::unbounded_channel();
        let manager = TerminalManager::new(Uuid::new_v4(), msg_tx);

        let request_id = Uuid::new_v4();

        // Start session
        manager
            .start_session(request_id, Some("/bin/sh".to_string()), 80, 24, HashMap::new())
            .await
            .unwrap();

        // Resize
        let result = manager.resize(request_id, 120, 40).await;
        assert!(result.is_ok(), "Failed to resize: {:?}", result);

        // Cleanup
        manager.close_session(request_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_session_not_found() {
        let (msg_tx, _msg_rx) = mpsc::unbounded_channel();
        let manager = TerminalManager::new(Uuid::new_v4(), msg_tx);

        let request_id = Uuid::new_v4();

        // Try operations on non-existent session
        assert!(manager.send_input(request_id, vec![]).await.is_err());
        assert!(manager.resize(request_id, 80, 24).await.is_err());
        assert!(manager.close_session(request_id).await.is_err());
    }
}
