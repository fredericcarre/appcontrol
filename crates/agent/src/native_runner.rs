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

use appcontrol_common::types::{NativeCommand, SnmpExpect};
use serde_json::json;

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
            bearer_token,
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
            // Bearer token is set first so an explicit `Authorization` entry
            // in `headers` (e.g. for non-bearer schemes like `Basic …`) wins.
            if let Some(token) = bearer_token {
                req = req.header("Authorization", format!("Bearer {token}"));
            }
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
        NativeCommand::Snmp {
            host,
            port,
            community,
            oid,
            expect,
            timeout_seconds,
        } => run_snmp(host, *port, community, oid, expect, *timeout_seconds, start).await,
    }
}

/// Execute one SNMPv2c GET and apply the configured expectation. Emits the
/// returned value as JSON on stdout so the metrics-extraction pipeline
/// picks it up; stderr carries the human-readable failure reason on
/// mismatch.
async fn run_snmp(
    host: &str,
    port: u16,
    community: &str,
    oid_str: &str,
    expect: &SnmpExpect,
    timeout_seconds: u32,
    start: Instant,
) -> ExecResult {
    let destination = format!("{host}:{port}");
    let timeout = Duration::from_secs(timeout_seconds as u64);

    let arcs = match parse_oid_string(oid_str) {
        Ok(a) => a,
        Err(e) => return snmp_error(start, format!("invalid OID '{oid_str}': {e}")),
    };
    let oid_obj = match snmp2::Oid::from(&arcs) {
        Ok(o) => o,
        Err(e) => return snmp_error(start, format!("OID '{oid_str}' rejected by ASN.1: {e:?}")),
    };

    let session_fut = snmp2::AsyncSession::new_v2c(destination.as_str(), community.as_bytes(), 1);
    let mut session = match tokio::time::timeout(timeout, session_fut).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            return snmp_error(start, format!("SNMP bind to {destination} failed: {e}"));
        }
        Err(_) => {
            return snmp_error(
                start,
                format!("SNMP bind to {destination} timed out after {timeout_seconds}s"),
            );
        }
    };

    let pdu = match tokio::time::timeout(timeout, session.get(&oid_obj)).await {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => {
            return snmp_error(
                start,
                format!("SNMP GET {oid_str} on {destination} failed: {e:?}"),
            );
        }
        Err(_) => {
            return snmp_error(
                start,
                format!("SNMP GET {oid_str} on {destination} timed out after {timeout_seconds}s"),
            );
        }
    };

    let (_, value) = match pdu.varbinds.clone().next() {
        Some(pair) => pair,
        None => {
            return snmp_error(
                start,
                format!("SNMP response for {oid_str} has no varbinds"),
            );
        }
    };

    let (type_name, str_value, numeric_value) = describe_snmp_value(&value);
    let (ok, reason) = evaluate_snmp_expect(expect, type_name, &str_value, numeric_value);

    let stdout = json!({
        "oid": oid_str,
        "type": type_name,
        "value": str_value,
        "numeric_value": numeric_value,
    })
    .to_string();

    ExecResult {
        exit_code: if ok { 0 } else { 1 },
        stdout,
        stderr: reason,
        duration_ms: start.elapsed().as_millis() as u32,
    }
}

fn snmp_error(start: Instant, msg: String) -> ExecResult {
    ExecResult {
        exit_code: 1,
        stdout: String::new(),
        stderr: msg,
        duration_ms: start.elapsed().as_millis() as u32,
    }
}

/// Split `1.3.6.1.2.1.1.5.0` into a `Vec<u32>`. Rejects empty input,
/// leading dots, and non-numeric arcs.
fn parse_oid_string(s: &str) -> Result<Vec<u64>, String> {
    if s.is_empty() {
        return Err("empty OID".to_string());
    }
    let trimmed = s.strip_prefix('.').unwrap_or(s);
    let arcs: Result<Vec<u64>, _> = trimmed.split('.').map(str::parse::<u64>).collect();
    let arcs = arcs.map_err(|e| format!("arc parse: {e}"))?;
    if arcs.len() < 2 {
        return Err("OID must have at least two arcs".to_string());
    }
    Ok(arcs)
}

/// Coerce an SNMP value into `(type_name, stringified, optional_f64)`.
pub(crate) fn describe_snmp_value(v: &snmp2::Value<'_>) -> (&'static str, String, Option<f64>) {
    use snmp2::Value;
    match v {
        Value::Integer(n) => ("Integer", n.to_string(), Some(*n as f64)),
        Value::Counter32(n) => ("Counter32", n.to_string(), Some(*n as f64)),
        Value::Counter64(n) => ("Counter64", n.to_string(), Some(*n as f64)),
        Value::Unsigned32(n) => ("Unsigned32", n.to_string(), Some(*n as f64)),
        Value::Timeticks(n) => ("Timeticks", n.to_string(), Some(*n as f64)),
        Value::OctetString(b) => ("OctetString", String::from_utf8_lossy(b).into_owned(), None),
        Value::ObjectIdentifier(oid) => ("OID", format!("{oid}"), None),
        Value::IpAddress([a, b, c, d]) => ("IpAddress", format!("{a}.{b}.{c}.{d}"), None),
        Value::Boolean(b) => ("Boolean", b.to_string(), Some(if *b { 1.0 } else { 0.0 })),
        Value::Null => ("Null", String::new(), None),
        Value::NoSuchObject => ("NoSuchObject", String::new(), None),
        Value::NoSuchInstance => ("NoSuchInstance", String::new(), None),
        Value::EndOfMibView => ("EndOfMibView", String::new(), None),
        Value::Opaque(b) => ("Opaque", format!("{} bytes", b.len()), None),
        _ => ("Other", format!("{v:?}"), None),
    }
}

/// Apply the configured expectation to the coerced SNMP value. Returns
/// `(passed, reason_on_failure)`.
pub(crate) fn evaluate_snmp_expect(
    expect: &SnmpExpect,
    type_name: &str,
    str_value: &str,
    numeric_value: Option<f64>,
) -> (bool, String) {
    // Special-case the "no such object / instance" responses: any
    // expectation against a missing OID is a failure, even `Present`.
    if matches!(
        type_name,
        "NoSuchObject" | "NoSuchInstance" | "EndOfMibView"
    ) {
        return (false, format!("OID resolved to {type_name}"));
    }

    match expect {
        SnmpExpect::Present => (true, String::new()),
        SnmpExpect::Equals { value } => {
            if str_value == value {
                (true, String::new())
            } else {
                (
                    false,
                    format!("expected value '{value}', got '{str_value}'"),
                )
            }
        }
        SnmpExpect::InRange { min, max } => match numeric_value {
            Some(n) if n >= *min && n <= *max => (true, String::new()),
            Some(n) => (false, format!("expected {min}..={max}, got {n}")),
            None => (
                false,
                format!("value is not numeric (type={type_name}, value='{str_value}')"),
            ),
        },
        SnmpExpect::Regex { pattern } => match regex::Regex::new(pattern) {
            Ok(re) if re.is_match(str_value) => (true, String::new()),
            Ok(_) => (
                false,
                format!("value '{str_value}' did not match regex /{pattern}/"),
            ),
            Err(e) => (false, format!("invalid regex /{pattern}/: {e}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_oid_simple() {
        let arcs = parse_oid_string("1.3.6.1.2.1.1.5.0").unwrap();
        assert_eq!(arcs, vec![1, 3, 6, 1, 2, 1, 1, 5, 0]);
    }

    #[test]
    fn parse_oid_strips_leading_dot() {
        assert_eq!(parse_oid_string(".1.3.6.1").unwrap(), vec![1, 3, 6, 1]);
    }

    #[test]
    fn parse_oid_rejects_empty() {
        assert!(parse_oid_string("").is_err());
    }

    #[test]
    fn parse_oid_rejects_single_arc() {
        assert!(parse_oid_string("1").is_err());
    }

    #[test]
    fn parse_oid_rejects_non_numeric() {
        assert!(parse_oid_string("1.3.foo.1").is_err());
    }

    #[test]
    fn expect_present_passes_for_any_value() {
        let (ok, _) = evaluate_snmp_expect(&SnmpExpect::Present, "Integer", "1", Some(1.0));
        assert!(ok);
        let (ok, _) = evaluate_snmp_expect(&SnmpExpect::Present, "OctetString", "hello", None);
        assert!(ok);
    }

    #[test]
    fn expect_present_fails_for_missing_oid() {
        let (ok, reason) = evaluate_snmp_expect(&SnmpExpect::Present, "NoSuchObject", "", None);
        assert!(!ok);
        assert!(reason.contains("NoSuchObject"));
    }

    #[test]
    fn expect_equals_string_match() {
        let exp = SnmpExpect::Equals {
            value: "router-prod-01".to_string(),
        };
        let (ok, _) = evaluate_snmp_expect(&exp, "OctetString", "router-prod-01", None);
        assert!(ok);
        let (ok, reason) = evaluate_snmp_expect(&exp, "OctetString", "router-prod-02", None);
        assert!(!ok);
        assert!(reason.contains("router-prod-01"));
        assert!(reason.contains("router-prod-02"));
    }

    #[test]
    fn expect_equals_integer_uses_stringified() {
        // ifOperStatus = 1 means "up"
        let exp = SnmpExpect::Equals {
            value: "1".to_string(),
        };
        let (ok, _) = evaluate_snmp_expect(&exp, "Integer", "1", Some(1.0));
        assert!(ok);
        let (ok, _) = evaluate_snmp_expect(&exp, "Integer", "2", Some(2.0));
        assert!(!ok);
    }

    #[test]
    fn expect_in_range_inclusive_bounds() {
        let exp = SnmpExpect::InRange {
            min: 0.0,
            max: 70.0,
        };
        let (ok, _) = evaluate_snmp_expect(&exp, "Integer", "0", Some(0.0));
        assert!(ok, "min bound inclusive");
        let (ok, _) = evaluate_snmp_expect(&exp, "Integer", "70", Some(70.0));
        assert!(ok, "max bound inclusive");
        let (ok, _) = evaluate_snmp_expect(&exp, "Integer", "35", Some(35.0));
        assert!(ok);
        let (ok, reason) = evaluate_snmp_expect(&exp, "Integer", "85", Some(85.0));
        assert!(!ok);
        assert!(reason.contains("85"));
    }

    #[test]
    fn expect_in_range_rejects_non_numeric() {
        let exp = SnmpExpect::InRange {
            min: 0.0,
            max: 100.0,
        };
        let (ok, reason) = evaluate_snmp_expect(&exp, "OctetString", "hello", None);
        assert!(!ok);
        assert!(reason.contains("not numeric"));
    }

    #[test]
    fn expect_regex_matches() {
        let exp = SnmpExpect::Regex {
            pattern: r".*prod.*".to_string(),
        };
        let (ok, _) = evaluate_snmp_expect(&exp, "OctetString", "router-prod-01", None);
        assert!(ok);
        let (ok, reason) = evaluate_snmp_expect(&exp, "OctetString", "router-dev-01", None);
        assert!(!ok);
        assert!(reason.contains("did not match"));
    }

    #[test]
    fn expect_regex_invalid_pattern_fails() {
        let exp = SnmpExpect::Regex {
            pattern: r"[unclosed".to_string(),
        };
        let (ok, reason) = evaluate_snmp_expect(&exp, "OctetString", "anything", None);
        assert!(!ok);
        assert!(reason.contains("invalid regex"));
    }

    #[test]
    fn describe_value_integer() {
        let v = snmp2::Value::Integer(42);
        let (t, s, n) = describe_snmp_value(&v);
        assert_eq!(t, "Integer");
        assert_eq!(s, "42");
        assert_eq!(n, Some(42.0));
    }

    #[test]
    fn describe_value_counter32() {
        let v = snmp2::Value::Counter32(123456);
        let (t, s, n) = describe_snmp_value(&v);
        assert_eq!(t, "Counter32");
        assert_eq!(s, "123456");
        assert_eq!(n, Some(123456.0));
    }

    #[test]
    fn describe_value_octet_string() {
        let v = snmp2::Value::OctetString(b"router-01");
        let (t, s, n) = describe_snmp_value(&v);
        assert_eq!(t, "OctetString");
        assert_eq!(s, "router-01");
        assert_eq!(n, None);
    }

    #[test]
    fn describe_value_ip_address() {
        let v = snmp2::Value::IpAddress([192, 168, 1, 10]);
        let (t, s, _) = describe_snmp_value(&v);
        assert_eq!(t, "IpAddress");
        assert_eq!(s, "192.168.1.10");
    }

    #[test]
    fn describe_value_no_such_object() {
        let v = snmp2::Value::NoSuchObject;
        let (t, _, _) = describe_snmp_value(&v);
        assert_eq!(t, "NoSuchObject");
    }

    // ----- Live-network integration tests (opt-in) -----
    //
    // These exercise the full UDP path through `run_snmp` against a real
    // SNMP responder. They are `#[ignore]` because CI does not bundle an
    // snmpd; run them locally with:
    //
    //     APPCONTROL_SNMP_TEST_TARGET=192.0.2.10:161 \
    //     APPCONTROL_SNMP_TEST_COMMUNITY=public \
    //     cargo test -p appcontrol-agent native_runner -- --ignored
    //
    // The target must respond to SNMPv2c GET on the standard
    // sysUpTime.0 OID (1.3.6.1.2.1.1.3.0).

    fn live_target() -> Option<(String, u16, String)> {
        let raw = std::env::var("APPCONTROL_SNMP_TEST_TARGET").ok()?;
        let community = std::env::var("APPCONTROL_SNMP_TEST_COMMUNITY")
            .unwrap_or_else(|_| "public".to_string());
        let (host, port) = match raw.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse::<u16>().unwrap_or(161)),
            None => (raw, 161),
        };
        Some((host, port, community))
    }

    #[tokio::test]
    #[ignore]
    async fn live_snmp_present_succeeds_on_sysuptime() {
        let (host, port, community) = match live_target() {
            Some(t) => t,
            None => {
                eprintln!("APPCONTROL_SNMP_TEST_TARGET not set; skipping");
                return;
            }
        };

        let result = run_snmp(
            &host,
            port,
            &community,
            "1.3.6.1.2.1.1.3.0",
            &SnmpExpect::Present,
            5,
            Instant::now(),
        )
        .await;

        assert_eq!(
            result.exit_code, 0,
            "Present check should succeed on sysUpTime.0; stderr: {}",
            result.stderr
        );
        let stdout: serde_json::Value =
            serde_json::from_str(&result.stdout).expect("stdout should be JSON from run_snmp");
        assert_eq!(stdout["oid"], "1.3.6.1.2.1.1.3.0");
        assert_eq!(stdout["type"], "Timeticks");
        assert!(stdout["numeric_value"].as_f64().unwrap() >= 0.0);
    }

    #[tokio::test]
    #[ignore]
    async fn live_snmp_in_range_passes_for_positive_uptime() {
        let (host, port, community) = match live_target() {
            Some(t) => t,
            None => return,
        };

        let result = run_snmp(
            &host,
            port,
            &community,
            "1.3.6.1.2.1.1.3.0",
            &SnmpExpect::InRange {
                min: 0.0,
                max: 1e18,
            },
            5,
            Instant::now(),
        )
        .await;
        assert_eq!(result.exit_code, 0, "stderr: {}", result.stderr);
    }

    #[tokio::test]
    #[ignore]
    async fn live_snmp_equals_fails_on_mismatch() {
        let (host, port, community) = match live_target() {
            Some(t) => t,
            None => return,
        };

        let result = run_snmp(
            &host,
            port,
            &community,
            "1.3.6.1.2.1.1.3.0",
            &SnmpExpect::Equals {
                value: "definitely-not-the-uptime".to_string(),
            },
            5,
            Instant::now(),
        )
        .await;
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("expected value"));
    }

    #[tokio::test]
    async fn unreachable_target_fails_within_timeout() {
        // RFC 5737 TEST-NET-1 address on a high port; nothing should respond.
        // Validates that the runner doesn't hang past its configured budget.
        let started = Instant::now();
        let result = run_snmp(
            "192.0.2.1",
            61161,
            "public",
            "1.3.6.1.2.1.1.3.0",
            &SnmpExpect::Present,
            2,
            Instant::now(),
        )
        .await;
        assert_eq!(result.exit_code, 1);
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "should fail within ~2s timeout, took {:?}",
            started.elapsed()
        );
    }
}
