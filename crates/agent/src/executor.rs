#[cfg(unix)]
use nix::unistd::{fork, setsid, ForkResult};
use std::process::Stdio;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Close inherited file descriptors (Unix only)
// ---------------------------------------------------------------------------

/// Close all file descriptors inherited from the parent process (FD >= 3).
///
/// This is CRITICAL for detached processes: without it, the child inherits
/// open handles to the agent's WebSocket, sled database, log files, etc.
/// These leaked FDs can:
/// - Prevent proper cleanup when the agent restarts
/// - Hold resources (sockets, files) open indefinitely
/// - Cause "address already in use" errors on agent restart
///
/// On Linux 5.9+, uses the efficient close_range() syscall.
/// On older systems/macOS, falls back to iterating FDs 3..max_fd.
#[cfg(unix)]
unsafe fn close_inherited_fds() {
    // Try close_range() first (Linux 5.9+, kernel syscall 436)
    #[cfg(target_os = "linux")]
    {
        // close_range(first, last, flags) - closes FDs from first to last inclusive
        // flags=0 means close normally (not CLOSE_RANGE_UNSHARE or CLOSE_RANGE_CLOEXEC)
        let result = libc::syscall(libc::SYS_close_range, 3_u32, u32::MAX, 0_u32);
        if result == 0 {
            return; // Success, all FDs >= 3 are closed
        }
        // Fall through to manual close if syscall not available
    }

    // Fallback: close FDs 3 to max manually
    // This works on all Unix systems but is slower for high FD counts
    let max_fd = libc::sysconf(libc::_SC_OPEN_MAX);
    let max_fd = if max_fd <= 0 { 1024 } else { max_fd as i32 };

    for fd in 3..max_fd {
        // close() on an invalid FD just returns EBADF, which we ignore
        libc::close(fd);
    }
}

/// Command execution mode.
#[allow(dead_code)]
pub enum CommandMode {
    /// Sync: agent waits for result (checks, diagnostics).
    Sync { timeout: Duration },
    /// Async: agent returns immediately, process is detached (start, stop, rebuild).
    Async,
}

/// Result of a synchronous command execution.
#[derive(Debug)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u32,
}

/// Execute a command synchronously with timeout.
///
/// On timeout, the child process (and its entire process group) is killed
/// to prevent orphaned processes from lingering after the agent moves on.
pub async fn execute_sync(command: &str, timeout: Duration) -> anyhow::Result<ExecResult> {
    let start = std::time::Instant::now();

    #[cfg(unix)]
    let child = {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0); // Create a new process group so we can kill all children
        cmd.spawn()?
    };

    #[cfg(windows)]
    let child = {
        use tokio::process::Command;
        // Detect PowerShell commands to avoid CMD mangling JSON output.
        // Extract the script portion after "powershell ... -Command" and run directly.
        let trimmed = command.trim_start();
        let lower = trimmed.to_lowercase();
        let mut cmd = if lower.starts_with("powershell") {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-NonInteractive"]);
            // Extract the -Command argument value from the command string
            if let Some(pos) = lower.find("-command") {
                let after_flag = &trimmed[pos + 8..].trim_start();
                c.args(["-Command", after_flag]);
            } else {
                // No -Command flag — pass everything after "powershell" as the command
                let after_ps = trimmed[10..].trim_start(); // skip "powershell"
                if !after_ps.is_empty() {
                    c.args(["-Command", after_ps]);
                }
            }
            c
        } else {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(command);
            c
        };
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // CREATE_NEW_PROCESS_GROUP allows us to kill the entire tree on timeout
            .creation_flags(windows::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP.0);
        cmd.spawn()?
    };

    // Capture the PID before wait_with_output() consumes the Child handle
    let child_pid = child.id();

    let result = tokio::time::timeout(timeout, child.wait_with_output()).await;
    let duration_ms = start.elapsed().as_millis() as u32;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            // Truncate to 4KB
            let stdout = if stdout.len() > 4096 {
                stdout[..4096].to_string()
            } else {
                stdout
            };
            let stderr = if stderr.len() > 4096 {
                stderr[..4096].to_string()
            } else {
                stderr
            };

            Ok(ExecResult {
                exit_code: output.status.code().unwrap_or(-1),
                stdout,
                stderr,
                duration_ms,
            })
        }
        Ok(Err(e)) => Err(e.into()),
        Err(_) => {
            // Timeout! Kill the process and its children
            tracing::warn!(command = %command, "Command timed out after {:?}, killing process", timeout);

            // Try SIGTERM first, then SIGKILL using the saved PID
            #[cfg(unix)]
            if let Some(pid) = child_pid {
                // Kill the process group (negative PID kills the group)
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGTERM);
                }
                // Give it 5 seconds to die gracefully
                tokio::time::sleep(Duration::from_secs(5)).await;
                // Force kill if still alive
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
            }

            // On Windows, terminate the process tree via job objects
            #[cfg(windows)]
            if let Some(pid) = child_pid {
                win_kill_process_tree(pid);
            }

            Ok(ExecResult {
                exit_code: -1,
                stdout: String::new(),
                stderr: "Command timed out and was killed".to_string(),
                duration_ms,
            })
        }
    }
}

/// Execute a command synchronously with streaming output chunks.
///
/// Similar to `execute_sync`, but sends stdout/stderr chunks via a callback
/// as they become available (every ~500ms or when buffer has data).
/// This enables real-time output streaming to the frontend.
pub async fn execute_sync_streaming<F>(
    command: &str,
    timeout: Duration,
    mut on_chunk: F,
) -> anyhow::Result<ExecResult>
where
    F: FnMut(String, String) + Send + 'static,
{
    use tokio::io::AsyncBufReadExt;
    let start = std::time::Instant::now();

    #[cfg(unix)]
    let mut child = {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        cmd.spawn()?
    };

    #[cfg(windows)]
    let mut child = {
        use tokio::process::Command;
        let trimmed = command.trim_start();
        let lower = trimmed.to_lowercase();
        let mut cmd = if lower.starts_with("powershell") {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-NonInteractive"]);
            if let Some(pos) = lower.find("-command") {
                let after_flag = &trimmed[pos + 8..].trim_start();
                c.args(["-Command", after_flag]);
            } else {
                let after_ps = trimmed[10..].trim_start();
                if !after_ps.is_empty() {
                    c.args(["-Command", after_ps]);
                }
            }
            c
        } else {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(command);
            c
        };
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(windows::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP.0);
        cmd.spawn()?
    };

    let child_pid = child.id();
    let child_stdout = child.stdout.take();
    let child_stderr = child.stderr.take();

    let mut all_stdout = String::new();
    let mut all_stderr = String::new();

    // Spawn readers for stdout and stderr
    let (stdout_tx, mut stdout_rx) = tokio::sync::mpsc::channel::<String>(64);
    let (stderr_tx, mut stderr_rx) = tokio::sync::mpsc::channel::<String>(64);

    if let Some(stdout) = child_stdout {
        let tx = stdout_tx;
        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send(line).await.is_err() {
                    break;
                }
            }
        });
    }

    if let Some(stderr) = child_stderr {
        let tx = stderr_tx;
        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send(line).await.is_err() {
                    break;
                }
            }
        });
    }

    // Collect output and send chunks periodically
    let deadline = tokio::time::Instant::now() + timeout;
    let mut chunk_interval = tokio::time::interval(Duration::from_millis(500));
    chunk_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut pending_stdout = String::new();
    let mut pending_stderr = String::new();
    let mut stdout_closed = false;
    let mut stderr_closed = false;

    loop {
        tokio::select! {
            line = stdout_rx.recv(), if !stdout_closed => {
                match line {
                    Some(l) => {
                        all_stdout.push_str(&l);
                        all_stdout.push('\n');
                        pending_stdout.push_str(&l);
                        pending_stdout.push('\n');
                    }
                    None => stdout_closed = true,
                }
            }
            line = stderr_rx.recv(), if !stderr_closed => {
                match line {
                    Some(l) => {
                        all_stderr.push_str(&l);
                        all_stderr.push('\n');
                        pending_stderr.push_str(&l);
                        pending_stderr.push('\n');
                    }
                    None => stderr_closed = true,
                }
            }
            _ = chunk_interval.tick() => {
                if !pending_stdout.is_empty() || !pending_stderr.is_empty() {
                    on_chunk(
                        std::mem::take(&mut pending_stdout),
                        std::mem::take(&mut pending_stderr),
                    );
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                // Timeout
                tracing::warn!(command = %command, "Streaming command timed out after {:?}", timeout);
                #[cfg(unix)]
                if let Some(pid) = child_pid {
                    unsafe { libc::kill(-(pid as i32), libc::SIGTERM); }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
                }
                #[cfg(windows)]
                if let Some(pid) = child_pid {
                    win_kill_process_tree(pid);
                }
                // Send remaining
                if !pending_stdout.is_empty() || !pending_stderr.is_empty() {
                    on_chunk(std::mem::take(&mut pending_stdout), std::mem::take(&mut pending_stderr));
                }
                let duration_ms = start.elapsed().as_millis() as u32;
                return Ok(ExecResult {
                    exit_code: -1,
                    stdout: truncate_output(all_stdout),
                    stderr: "Command timed out and was killed".to_string(),
                    duration_ms,
                });
            }
        }

        // Check if child has exited and streams are closed
        if stdout_closed && stderr_closed {
            break;
        }
    }

    // Send any remaining output
    if !pending_stdout.is_empty() || !pending_stderr.is_empty() {
        on_chunk(
            std::mem::take(&mut pending_stdout),
            std::mem::take(&mut pending_stderr),
        );
    }

    let status = child.wait().await?;
    let duration_ms = start.elapsed().as_millis() as u32;

    Ok(ExecResult {
        exit_code: status.code().unwrap_or(-1),
        stdout: truncate_output(all_stdout),
        stderr: truncate_output(all_stderr),
        duration_ms,
    })
}

fn truncate_output(s: String) -> String {
    if s.len() > 4096 {
        s[..4096].to_string()
    } else {
        s
    }
}

/// Execute a command asynchronously with double-fork + setsid for process detachment.
///
/// CRITICAL: The spawned process MUST survive agent crash.
///
/// Algorithm:
/// 1. fork() → child
/// 2. In child: setsid() → new session
/// 3. fork() again → grandchild
/// 4. Intermediate child exits immediately
/// 5. Grandchild: close file descriptors, redirect to /dev/null, apply resource limits
/// 6. Grandchild: exec() the command
///
/// Result: grandchild is reparented to init/PID 1
#[cfg(unix)]
pub fn execute_async_detached(command: &str) -> anyhow::Result<u32> {
    // Use a pipe to communicate the grandchild PID back (using libc directly)
    let mut pipe_fds = [0i32; 2];
    if unsafe { libc::pipe(pipe_fds.as_mut_ptr()) } != 0 {
        return Err(anyhow::anyhow!("pipe() failed"));
    }
    let read_fd = pipe_fds[0];
    let write_fd = pipe_fds[1];

    match unsafe { fork()? } {
        ForkResult::Parent { child } => {
            // Parent: close write end, read grandchild PID
            unsafe {
                libc::close(write_fd);
            }

            let mut buf = [0u8; 4];
            unsafe {
                libc::read(read_fd, buf.as_mut_ptr() as *mut libc::c_void, 4);
            }
            unsafe {
                libc::close(read_fd);
            }

            // Wait for intermediate child to exit
            nix::sys::wait::waitpid(child, None)?;

            let pid = u32::from_le_bytes(buf);
            Ok(pid)
        }
        ForkResult::Child => {
            // Intermediate child
            unsafe {
                libc::close(read_fd);
            }

            // Create new session
            setsid().ok();

            match unsafe { fork() } {
                Ok(ForkResult::Parent { child }) => {
                    // Write grandchild PID to parent
                    let pid_bytes = (child.as_raw() as u32).to_le_bytes();
                    unsafe {
                        libc::write(write_fd, pid_bytes.as_ptr() as *const libc::c_void, 4);
                    }
                    unsafe {
                        libc::close(write_fd);
                    }
                    // Intermediate child exits
                    std::process::exit(0);
                }
                Ok(ForkResult::Child) => {
                    // Grandchild: detached process
                    unsafe {
                        libc::close(write_fd);
                    }

                    // Redirect stdin/stdout/stderr to /dev/null
                    let devnull = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDWR) };
                    if devnull >= 0 {
                        unsafe {
                            libc::dup2(devnull, 0); // stdin
                            libc::dup2(devnull, 1); // stdout
                            libc::dup2(devnull, 2); // stderr
                            libc::close(devnull);
                        }
                    }

                    // Close all inherited file descriptors (FD >= 3)
                    // This prevents the detached process from holding agent's
                    // WebSocket, sled DB, and other handles open after fork.
                    unsafe {
                        close_inherited_fds();
                    }

                    // NOTE: We intentionally do NOT apply resource limits to detached processes.
                    // These are async commands (start/stop/restart) that can run for hours
                    // (database backups, deployments, rebuilds). The agent returns immediately
                    // and doesn't wait for them, so limiting CPU time or memory would just
                    // cause arbitrary failures. Let the OS and the commands themselves
                    // manage their resources.

                    // Use absolute path to sh for reliability
                    let err = exec::execvp("/bin/sh", &["/bin/sh", "-c", command]);
                    // If exec failed, write error to a debug file
                    if let Ok(mut f) = std::fs::File::create("/tmp/detached_exec_error.log") {
                        use std::io::Write;
                        let _ = writeln!(f, "exec failed: {:?}", err);
                    }
                    std::process::exit(127);
                }
                Err(_) => {
                    std::process::exit(1);
                }
            }
        }
    }
}

#[cfg(unix)]
mod exec {
    use std::ffi::CString;

    pub fn execvp(program: &str, args: &[&str]) -> std::io::Error {
        let program = CString::new(program).unwrap();
        let args: Vec<CString> = args.iter().map(|a| CString::new(*a).unwrap()).collect();
        let arg_ptrs: Vec<*const libc::c_char> = args
            .iter()
            .map(|a| a.as_ptr())
            .chain(std::iter::once(std::ptr::null()))
            .collect();

        unsafe {
            libc::execvp(program.as_ptr(), arg_ptrs.as_ptr());
        }
        std::io::Error::last_os_error()
    }
}

// ---------------------------------------------------------------------------
// Windows: detached process execution
// ---------------------------------------------------------------------------

/// Execute a command asynchronously on Windows with process detachment.
///
/// CRITICAL: The spawned process MUST survive agent crash.
///
/// Uses CreateProcess with CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS flags.
/// The process runs independently of the agent — no console, no parent dependency.
/// A Windows Job Object is NOT used here (we WANT the process to survive agent exit).
#[cfg(windows)]
pub fn execute_async_detached(command: &str) -> anyhow::Result<u32> {
    use std::os::windows::process::CommandExt;

    // DETACHED_PROCESS: no console window
    // CREATE_NEW_PROCESS_GROUP: own Ctrl+C group, survives agent shutdown
    // CREATE_BREAKAWAY_FROM_JOB: escape any job object the agent might be in
    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x01000000;

    let child = std::process::Command::new("cmd")
        .args(["/C", command])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB)
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn detached process: {}", e))?;

    let pid = child.id();
    tracing::info!(pid = pid, "Detached process launched on Windows");

    // We intentionally do NOT wait on the child — it must outlive the agent.
    // The Child handle is dropped here; on Windows this does NOT kill the process.
    Ok(pid)
}

/// Kill a process and all its children on Windows using TerminateProcess.
#[cfg(windows)]
fn win_kill_process_tree(pid: u32) {
    // Use taskkill /T /F which kills the process tree
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(unix)]
    async fn test_execute_sync_success() {
        let result = execute_sync("echo hello", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn test_execute_sync_success_windows() {
        let result = execute_sync("echo hello", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.trim().contains("hello"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_execute_sync_failure() {
        let result = execute_sync("exit 42", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(result.exit_code, 42);
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn test_execute_sync_failure_windows() {
        let result = execute_sync("exit /B 42", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(result.exit_code, 42);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_execute_sync_timeout() {
        let result = execute_sync("sleep 60", Duration::from_millis(500))
            .await
            .unwrap();
        assert_eq!(result.exit_code, -1);
        assert!(result.stderr.contains("timed out"));
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn test_execute_sync_timeout_windows() {
        // ping -n 60 localhost takes ~60 seconds
        let result = execute_sync("ping -n 60 127.0.0.1", Duration::from_millis(500))
            .await
            .unwrap();
        assert_eq!(result.exit_code, -1);
        assert!(result.stderr.contains("timed out"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_execute_sync_timeout_kills_process() {
        // Start a process that runs for a long time; timeout should kill it
        let result = execute_sync("sleep 60", Duration::from_millis(500))
            .await
            .unwrap();
        assert_eq!(result.exit_code, -1);
        assert!(result.stderr.contains("killed"));
    }

    #[test]
    #[cfg(unix)]
    fn test_execute_async_detached() {
        let pid = execute_async_detached("sleep 5").unwrap();
        assert!(pid > 0);

        std::thread::sleep(Duration::from_millis(500));

        // Verify process exists
        let exists = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok();
        assert!(exists, "Detached process should be running");

        // Clean up
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGTERM,
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_execute_async_detached_creates_file() {
        let test_file = "/tmp/test_detached_file_creation.flag";
        // Clean up from previous runs
        let _ = std::fs::remove_file(test_file);

        let cmd = format!("touch {} && echo 'created'", test_file);
        let pid = execute_async_detached(&cmd).unwrap();
        assert!(pid > 0, "Should return a valid PID");

        // Wait for the command to complete
        std::thread::sleep(Duration::from_secs(2));

        // Verify the file was created
        assert!(
            std::path::Path::new(test_file).exists(),
            "Detached process should create the file"
        );

        // Clean up
        let _ = std::fs::remove_file(test_file);
    }

    #[test]
    #[cfg(windows)]
    fn test_execute_async_detached_windows() {
        // Launch a short-lived detached process
        let pid = execute_async_detached("timeout /T 2 /NOBREAK >nul").unwrap();
        assert!(pid > 0);
        // We can't easily check if the process is running without OpenProcess,
        // but if we got a PID back, the launch succeeded.
    }
}
