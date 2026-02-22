#[cfg(unix)]
use nix::unistd::{fork, setsid, ForkResult};
use std::process::Stdio;
use std::time::Duration;

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
pub async fn execute_sync(command: &str, timeout: Duration) -> anyhow::Result<ExecResult> {
    let start = std::time::Instant::now();

    let result = tokio::time::timeout(timeout, async {
        #[cfg(unix)]
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        #[cfg(windows)]
        let output = tokio::process::Command::new("cmd")
            .arg("/C")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        Ok::<_, anyhow::Error>(output)
    })
    .await;

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
        Ok(Err(e)) => Err(e),
        Err(_) => Ok(ExecResult {
            exit_code: -1,
            stdout: String::new(),
            stderr: "Command timed out".to_string(),
            duration_ms,
        }),
    }
}

/// Apply resource limits to the current process (called in child before exec).
///
/// Prevents runaway commands from exhausting host resources.
/// These limits are inherited by exec'd processes.
#[cfg(unix)]
fn apply_resource_limits() {
    unsafe {
        // CPU time limit: 30 seconds soft, 120 seconds hard
        let cpu_limit = libc::rlimit {
            rlim_cur: 30,
            rlim_max: 120,
        };
        libc::setrlimit(libc::RLIMIT_CPU, &cpu_limit);

        // Virtual memory limit: 512 MB soft, 1 GB hard
        let mem_limit = libc::rlimit {
            rlim_cur: 512 * 1024 * 1024,
            rlim_max: 1024 * 1024 * 1024,
        };
        libc::setrlimit(libc::RLIMIT_AS, &mem_limit);

        // File descriptor limit: 512 soft, 1024 hard
        let fd_limit = libc::rlimit {
            rlim_cur: 512,
            rlim_max: 1024,
        };
        libc::setrlimit(libc::RLIMIT_NOFILE, &fd_limit);

        // Child process limit: 64 soft, 128 hard
        let nproc_limit = libc::rlimit {
            rlim_cur: 64,
            rlim_max: 128,
        };
        libc::setrlimit(libc::RLIMIT_NPROC, &nproc_limit);
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

                    // Apply resource limits before exec
                    apply_resource_limits();

                    // Exec the command
                    let err = exec::execvp("sh", &["sh", "-c", command]);
                    eprintln!("exec failed: {:?}", err);
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
    #[cfg(unix)]
    async fn test_execute_sync_failure() {
        let result = execute_sync("exit 42", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(result.exit_code, 42);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_execute_sync_timeout() {
        let result = execute_sync("sleep 60", Duration::from_millis(100))
            .await
            .unwrap();
        assert_eq!(result.exit_code, -1);
        assert!(result.stderr.contains("timed out"));
    }

    #[test]
    #[cfg(unix)]
    fn test_execute_async_detached() {
        let pid = execute_async_detached("sleep 1").unwrap();
        assert!(pid > 0);

        std::thread::sleep(Duration::from_millis(100));

        // Verify process exists
        let exists = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok();
        assert!(exists, "Detached process should be running");

        // Clean up
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGTERM,
        );
    }
}
