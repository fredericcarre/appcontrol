//! Runner for typed (non-shell) commands.
//!
//! Lets the backend describe a check / start / stop as `NativeCommand::Http`
//! or `NativeCommand::TcpConnect` instead of a shell string — useful when
//! the host can't reasonably ship curl/wget (Windows is the typical pain
//! point) and to avoid per-host quoting headaches.
//!
//! The result shape (exit_code / stdout / stderr / duration_ms) matches
//! `executor::ExecResult` so the rest of the agent's pipeline doesn't care
//! whether a command was shell or native.

use std::time::{Duration, Instant};

use appcontrol_common::types::NativeCommand;

use crate::executor::ExecResult;

/// Run a typed native command. Maps the result to the same `ExecResult`
/// shape produced by shell commands so the caller can treat both paths
/// uniformly. Errors during the probe (DNS, connect, TLS) are reported as
/// `exit_code = 1` (i.e. "unhealthy"), with the cause in `stderr`.
pub async fn run(cmd: &NativeCommand) -> ExecResult {
    let start = Instant::now();
    match cmd {
        NativeCommand::Http {
            method,
            url,
            headers,
            body,
            expect_status,
            expect_body_contains,
            timeout_seconds,
            insecure,
        } => {
            let client_builder = reqwest::Client::builder()
                .timeout(Duration::from_secs(*timeout_seconds as u64))
                .danger_accept_invalid_certs(*insecure);
            let client = match client_builder.build() {
                Ok(c) => c,
                Err(e) => {
                    return ExecResult {
                        exit_code: 1,
                        stdout: String::new(),
                        stderr: format!("Failed to build HTTP client: {e}"),
                        duration_ms: start.elapsed().as_millis() as u32,
                    };
                }
            };
            let method_parsed = match reqwest::Method::from_bytes(method.as_bytes()) {
                Ok(m) => m,
                Err(e) => {
                    return ExecResult {
                        exit_code: 1,
                        stdout: String::new(),
                        stderr: format!("Invalid HTTP method '{method}': {e}"),
                        duration_ms: start.elapsed().as_millis() as u32,
                    };
                }
            };
            let mut req = client.request(method_parsed, url);
            for (k, v) in headers {
                req = req.header(k, v);
            }
            if let Some(b) = body {
                req = req.body(b.clone());
            }
            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    return ExecResult {
                        exit_code: 1,
                        stdout: String::new(),
                        stderr: format!("HTTP request failed: {e}"),
                        duration_ms: start.elapsed().as_millis() as u32,
                    };
                }
            };
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();

            // Status check: explicit expectation if set, otherwise default to
            // "any 2xx is healthy".
            let status_ok = match expect_status {
                Some(want) => status.as_u16() == *want,
                None => status.is_success(),
            };
            let body_ok = match expect_body_contains {
                Some(needle) => body_text.contains(needle),
                None => true,
            };
            let exit_code = if status_ok && body_ok { 0 } else { 1 };

            // Truncate stdout to 4 KB to match the shell path's contract.
            let stdout = if body_text.len() > 4096 {
                body_text[..4096].to_string()
            } else {
                body_text
            };
            let stderr = if status_ok && body_ok {
                String::new()
            } else if !status_ok {
                format!(
                    "HTTP status mismatch: got {} expected {}",
                    status.as_u16(),
                    expect_status
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "2xx".to_string())
                )
            } else {
                format!(
                    "HTTP body did not contain expected substring '{}'",
                    expect_body_contains.as_deref().unwrap_or("")
                )
            };
            ExecResult {
                exit_code,
                stdout,
                stderr,
                duration_ms: start.elapsed().as_millis() as u32,
            }
        }
        NativeCommand::TcpConnect {
            host,
            port,
            timeout_seconds,
        } => {
            let addr = format!("{host}:{port}");
            let connect = tokio::net::TcpStream::connect(&addr);
            let result =
                tokio::time::timeout(Duration::from_secs(*timeout_seconds as u64), connect).await;
            let duration_ms = start.elapsed().as_millis() as u32;
            match result {
                Ok(Ok(_)) => ExecResult {
                    exit_code: 0,
                    stdout: format!("TCP connect to {addr} succeeded"),
                    stderr: String::new(),
                    duration_ms,
                },
                Ok(Err(e)) => ExecResult {
                    exit_code: 1,
                    stdout: String::new(),
                    stderr: format!("TCP connect to {addr} failed: {e}"),
                    duration_ms,
                },
                Err(_) => ExecResult {
                    exit_code: 1,
                    stdout: String::new(),
                    stderr: format!("TCP connect to {addr} timed out after {timeout_seconds}s"),
                    duration_ms,
                },
            }
        }
    }
}
