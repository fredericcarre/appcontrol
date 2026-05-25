//! SNMP v1/v2c trap receiver.
//!
//! Pairs with the SNMP poller in `native_runner`: the poller is pull (we
//! ask the device "are you up?"), the trap receiver is push (the device
//! tells us "I just rebooted"). Both feed the same `CheckResult` pipeline
//! and therefore the same FSM.
//!
//! The receiver is **opt-in** (`agent.yaml > snmp_traps.enabled: true`)
//! because:
//!   * port 162 is privileged on Unix — running the agent as root just
//!     to receive traps is not always acceptable (we default to 1162);
//!   * routing rules must be configured to know which trap belongs to
//!     which AppControl component.
//!
//! v3 (authPriv) traps are deliberately not handled in this MVP — they
//! require the crypto feature which clashes with other workspace deps.
//! v1 and v2c (community-string) traps cover the common enterprise
//! case (routers/UPS/storage already on a trusted management VLAN).

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use appcontrol_common::{AgentMessage, CheckResult, CheckType};
use serde_json::json;
use snmp2::{MessageType, Pdu, Value};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use crate::config::{SnmpTrapRoute, SnmpTrapsSection};

/// Maximum UDP datagram we'll accept. RFC 3411 caps SNMP messages at
/// 65 507 bytes; in practice traps are well under 1500.
const TRAP_BUFFER_BYTES: usize = 65_507;

/// Bind a UDP socket and spawn the listener loop. Returns an error if
/// the bind fails (port already in use, insufficient privileges, ...);
/// the caller should log and continue rather than abort the agent —
/// trap receiving is best-effort, not load-bearing.
pub async fn start(
    config: SnmpTrapsSection,
    msg_tx: UnboundedSender<AgentMessage>,
) -> std::io::Result<tokio::task::JoinHandle<()>> {
    let socket = UdpSocket::bind(&config.listen_addr).await?;
    tracing::info!(
        listen_addr = %config.listen_addr,
        route_count = config.routes.len(),
        "SNMP trap receiver bound"
    );
    let routes = Arc::new(config.routes);
    Ok(tokio::spawn(async move {
        listen_loop(socket, routes, msg_tx).await;
    }))
}

async fn listen_loop(
    socket: UdpSocket,
    routes: Arc<Vec<SnmpTrapRoute>>,
    msg_tx: UnboundedSender<AgentMessage>,
) {
    let mut buf = vec![0u8; TRAP_BUFFER_BYTES];
    loop {
        let (len, src) = match socket.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "SNMP trap recv failed");
                continue;
            }
        };

        match handle_datagram(&buf[..len], src, &routes) {
            Some(result) => {
                if msg_tx.send(AgentMessage::CheckResult(result)).is_err() {
                    tracing::error!("SNMP trap: outbound channel closed, stopping listener");
                    return;
                }
            }
            None => {
                tracing::debug!(src = %src, "SNMP trap not routed");
            }
        }
    }
}

/// Decode one UDP datagram and, if it matches a route, return the
/// `CheckResult` to forward. Pure function — fully unit-testable without
/// a real socket.
pub(crate) fn handle_datagram(
    bytes: &[u8],
    src: SocketAddr,
    routes: &[SnmpTrapRoute],
) -> Option<CheckResult> {
    let pdu = Pdu::from_bytes(bytes).ok()?;
    if !matches!(pdu.message_type, MessageType::Trap | MessageType::TrapV1) {
        return None;
    }

    let (trap_oid, varbinds_json) = extract_trap_oid_and_varbinds(&pdu);
    let source_ip = src.ip().to_string();

    let route = match_route(routes, &source_ip, &trap_oid)?;
    let component_id = Uuid::from_str(&route.component_id).ok()?;

    let stdout = json!({
        "source_ip": source_ip,
        "trap_oid": trap_oid,
        "route": route.name,
        "varbinds": varbinds_json,
    })
    .to_string();

    Some(CheckResult {
        component_id,
        check_type: CheckType::SnmpTrap,
        exit_code: route.exit_code,
        stdout: Some(stdout),
        duration_ms: 0,
        at: chrono::Utc::now(),
        metrics: None,
        cluster_member_id: None,
    })
}

/// Pull the trap OID (snmpTrapOID.0 in v2c, enterprise+specific in v1)
/// out of the PDU, plus a JSON-friendly varbinds list.
fn extract_trap_oid_and_varbinds(pdu: &Pdu) -> (String, Vec<serde_json::Value>) {
    // snmpTrapOID.0 = 1.3.6.1.6.3.1.1.4.1.0
    const SNMP_TRAP_OID: &str = "1.3.6.1.6.3.1.1.4.1.0";

    let mut trap_oid = String::new();
    let mut json_binds = Vec::new();

    for (oid, value) in pdu.varbinds.clone() {
        let oid_str = format!("{oid}");
        let value_str = stringify_value(&value);

        if oid_str == SNMP_TRAP_OID {
            // In v2c the value of snmpTrapOID.0 is itself an OID.
            if let Value::ObjectIdentifier(ref v) = value {
                trap_oid = format!("{v}");
            }
        }

        json_binds.push(json!({
            "oid": oid_str,
            "value": value_str,
        }));
    }

    // v1 traps carry the trap OID in `v1_trap_info` instead of a varbind.
    if trap_oid.is_empty() {
        if let Some(t) = &pdu.v1_trap_info {
            // Compose enterprise.specific OID following RFC 1907 mapping.
            // For generic traps (0..5) the OID is snmpTraps + (generic+1);
            // for enterprise-specific (generic=6) it's <enterprise>.0.<specific>.
            trap_oid = if t.generic_trap == 6 {
                format!("{}.0.{}", t.enterprise, t.specific_trap)
            } else {
                format!("1.3.6.1.6.3.1.1.5.{}", t.generic_trap + 1)
            };
        }
    }

    (trap_oid, json_binds)
}

fn stringify_value(v: &Value) -> String {
    match v {
        Value::Integer(n) => n.to_string(),
        Value::Counter32(n) => n.to_string(),
        Value::Counter64(n) => n.to_string(),
        Value::Unsigned32(n) => n.to_string(),
        Value::Timeticks(n) => n.to_string(),
        Value::OctetString(b) => String::from_utf8_lossy(b).into_owned(),
        Value::ObjectIdentifier(o) => format!("{o}"),
        Value::IpAddress([a, b, c, d]) => format!("{a}.{b}.{c}.{d}"),
        Value::Boolean(b) => b.to_string(),
        Value::Null => String::new(),
        Value::NoSuchObject => "NoSuchObject".into(),
        Value::NoSuchInstance => "NoSuchInstance".into(),
        Value::EndOfMibView => "EndOfMibView".into(),
        Value::Opaque(b) => format!("Opaque<{} bytes>", b.len()),
        _ => "Other".into(),
    }
}

/// Find the first route whose `source_host` and `oid_prefix` match. A
/// route with `source_host = "*"` matches any source; an empty
/// `oid_prefix` matches any OID.
pub(crate) fn match_route<'a>(
    routes: &'a [SnmpTrapRoute],
    source_ip: &str,
    trap_oid: &str,
) -> Option<&'a SnmpTrapRoute> {
    routes.iter().find(|r| {
        let host_ok = r.source_host == "*" || r.source_host == source_ip;
        let oid_ok = r.oid_prefix.is_empty() || trap_oid.starts_with(&r.oid_prefix);
        host_ok && oid_ok
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(name: &str, source: &str, prefix: &str, exit_code: i32) -> SnmpTrapRoute {
        SnmpTrapRoute {
            name: name.to_string(),
            source_host: source.to_string(),
            oid_prefix: prefix.to_string(),
            component_id: Uuid::new_v4().to_string(),
            exit_code,
        }
    }

    #[test]
    fn match_route_wildcard_source() {
        let routes = vec![route("r1", "*", "", 2)];
        let m = match_route(&routes, "10.0.0.1", "1.3.6.1");
        assert!(m.is_some());
    }

    #[test]
    fn match_route_exact_source_matches() {
        let routes = vec![route("r1", "10.0.0.1", "", 2)];
        assert!(match_route(&routes, "10.0.0.1", "x").is_some());
        assert!(match_route(&routes, "10.0.0.2", "x").is_none());
    }

    #[test]
    fn match_route_oid_prefix_filter() {
        let routes = vec![route("link-down", "*", "1.3.6.1.6.3.1.1.5.3", 2)];
        assert!(match_route(&routes, "10.0.0.1", "1.3.6.1.6.3.1.1.5.3").is_some());
        assert!(match_route(&routes, "10.0.0.1", "1.3.6.1.6.3.1.1.5.4").is_none());
    }

    #[test]
    fn match_route_first_wins() {
        let routes = vec![
            route("specific", "10.0.0.1", "1.3.6.1.4.1.9", 2),
            route("catch-all", "*", "", 0),
        ];
        let m = match_route(&routes, "10.0.0.1", "1.3.6.1.4.1.9.10").unwrap();
        assert_eq!(m.name, "specific");
        let m = match_route(&routes, "10.0.0.2", "1.3.6.1.6.3").unwrap();
        assert_eq!(m.name, "catch-all");
    }

    #[test]
    fn match_route_no_match_returns_none() {
        let routes = vec![route("r1", "10.0.0.1", "1.3", 2)];
        assert!(match_route(&routes, "10.0.0.2", "2.4").is_none());
    }

    /// Hand-crafted SNMPv2c "coldStart" trap: minimal valid PDU that
    /// snmp2 must parse, with snmpTrapOID.0 set to 1.3.6.1.6.3.1.1.5.1.
    fn coldstart_trap_v2c() -> Vec<u8> {
        // ASN.1 BER for:
        //   SEQUENCE {
        //     INTEGER 1                           -- version v2c
        //     OCTET STRING "public"               -- community
        //     [7] IMPLICIT SEQUENCE {             -- Trap-PDU (tag 0xa7)
        //       INTEGER 0                         -- request-id
        //       INTEGER 0                         -- error-status
        //       INTEGER 0                         -- error-index
        //       SEQUENCE {                        -- varbinds
        //         SEQUENCE {                      -- vb1: sysUpTime.0 = 0
        //           OID 1.3.6.1.2.1.1.3.0
        //           TIMETICKS 0
        //         }
        //         SEQUENCE {                      -- vb2: snmpTrapOID.0 = coldStart
        //           OID 1.3.6.1.6.3.1.1.4.1.0
        //           OID 1.3.6.1.6.3.1.1.5.1
        //         }
        //       }
        //     }
        //   }
        vec![
            0x30, 0x40, // SEQUENCE, len 64
            0x02, 0x01, 0x01, // INTEGER 1 (v2c)
            0x04, 0x06, b'p', b'u', b'b', b'l', b'i', b'c', // OCTET STRING "public"
            0xa7, 0x33, // Trap-PDU [7] IMPLICIT SEQUENCE, len 51
            0x02, 0x01, 0x00, // request-id 0
            0x02, 0x01, 0x00, // error-status 0
            0x02, 0x01, 0x00, // error-index 0
            0x30, 0x28, // varbinds SEQUENCE, len 40
            // vb1: sysUpTime.0 (1.3.6.1.2.1.1.3.0) = Timeticks 0     [len 13]
            0x30, 0x0d, // vb1 SEQUENCE, len 13
            0x06, 0x08, 0x2b, 0x06, 0x01, 0x02, 0x01, 0x01, 0x03,
            0x00, // OID 1.3.6.1.2.1.1.3.0
            0x43, 0x01, 0x00, // Timeticks 0 (tag 0x43 = application 3)
            // vb2: snmpTrapOID.0 (1.3.6.1.6.3.1.1.4.1.0) = OID 1.3.6.1.6.3.1.1.5.1 (coldStart)  [len 23]
            0x30, 0x17, // vb2 SEQUENCE, len 23
            0x06, 0x0a, 0x2b, 0x06, 0x01, 0x06, 0x03, 0x01, 0x01, 0x04, 0x01,
            0x00, // OID 1.3.6.1.6.3.1.1.4.1.0 (10 bytes)
            0x06, 0x09, 0x2b, 0x06, 0x01, 0x06, 0x03, 0x01, 0x01, 0x05,
            0x01, // OID 1.3.6.1.6.3.1.1.5.1 (9 bytes)
        ]
    }

    #[test]
    fn coldstart_trap_parses() {
        let bytes = coldstart_trap_v2c();
        let pdu = Pdu::from_bytes(&bytes).expect("trap should decode");
        assert!(matches!(pdu.message_type, MessageType::Trap));
        let (trap_oid, varbinds) = extract_trap_oid_and_varbinds(&pdu);
        assert_eq!(trap_oid, "1.3.6.1.6.3.1.1.5.1");
        assert_eq!(varbinds.len(), 2);
        assert_eq!(varbinds[0]["oid"], "1.3.6.1.2.1.1.3.0");
        assert_eq!(varbinds[1]["oid"], "1.3.6.1.6.3.1.1.4.1.0");
    }

    #[test]
    fn handle_datagram_routes_to_component() {
        let bytes = coldstart_trap_v2c();
        let comp_id = Uuid::new_v4();
        let routes = vec![SnmpTrapRoute {
            name: "any-trap".into(),
            source_host: "*".into(),
            oid_prefix: "1.3.6.1.6.3.1.1.5".into(),
            component_id: comp_id.to_string(),
            exit_code: 2,
        }];
        let src: SocketAddr = "10.1.2.3:1162".parse().unwrap();
        let res = handle_datagram(&bytes, src, &routes).expect("should route");
        assert_eq!(res.component_id, comp_id);
        assert_eq!(res.exit_code, 2);
        assert!(matches!(res.check_type, CheckType::SnmpTrap));
        let stdout: serde_json::Value = serde_json::from_str(res.stdout.as_ref().unwrap()).unwrap();
        assert_eq!(stdout["source_ip"], "10.1.2.3");
        assert_eq!(stdout["trap_oid"], "1.3.6.1.6.3.1.1.5.1");
        assert_eq!(stdout["route"], "any-trap");
    }

    #[test]
    fn handle_datagram_skips_unrouted_traps() {
        let bytes = coldstart_trap_v2c();
        let routes = vec![SnmpTrapRoute {
            name: "different-oid".into(),
            source_host: "*".into(),
            oid_prefix: "1.3.6.1.4.1.9999".into(),
            component_id: Uuid::new_v4().to_string(),
            exit_code: 2,
        }];
        let src: SocketAddr = "10.1.2.3:1162".parse().unwrap();
        assert!(handle_datagram(&bytes, src, &routes).is_none());
    }

    #[test]
    fn handle_datagram_skips_non_traps() {
        // Build a GET request — not a trap, should not be processed.
        let get_request = vec![
            0x30, 0x26, 0x02, 0x01, 0x01, 0x04, 0x06, b'p', b'u', b'b', b'l', b'i', b'c', 0xa0,
            0x19, 0x02, 0x01, 0x01, 0x02, 0x01, 0x00, 0x02, 0x01, 0x00, 0x30, 0x0e, 0x30, 0x0c,
            0x06, 0x08, 0x2b, 0x06, 0x01, 0x02, 0x01, 0x01, 0x03, 0x00, 0x05, 0x00,
        ];
        let routes = vec![SnmpTrapRoute {
            name: "any".into(),
            source_host: "*".into(),
            oid_prefix: "".into(),
            component_id: Uuid::new_v4().to_string(),
            exit_code: 2,
        }];
        let src: SocketAddr = "10.1.2.3:1162".parse().unwrap();
        assert!(handle_datagram(&get_request, src, &routes).is_none());
    }

    #[test]
    fn handle_datagram_skips_malformed_bytes() {
        let routes = vec![SnmpTrapRoute {
            name: "any".into(),
            source_host: "*".into(),
            oid_prefix: "".into(),
            component_id: Uuid::new_v4().to_string(),
            exit_code: 2,
        }];
        let src: SocketAddr = "10.1.2.3:1162".parse().unwrap();
        assert!(handle_datagram(&[0xff, 0xff, 0xff], src, &routes).is_none());
    }

    #[tokio::test]
    async fn end_to_end_listener_forwards_check_result() {
        // Bind on an ephemeral port, send a trap, verify the receiver
        // emits a CheckResult on the outbound channel.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let comp_id = Uuid::new_v4();
        let cfg = SnmpTrapsSection {
            enabled: true,
            listen_addr: "127.0.0.1:0".to_string(),
            routes: vec![SnmpTrapRoute {
                name: "test".to_string(),
                source_host: "*".to_string(),
                oid_prefix: "".to_string(),
                component_id: comp_id.to_string(),
                exit_code: 2,
            }],
        };

        // We need to know which port the OS picked, so bind manually and
        // hand the socket off (skips the helper but tests the loop body).
        let socket = UdpSocket::bind(&cfg.listen_addr).await.unwrap();
        let bound_addr = socket.local_addr().unwrap();
        let routes = Arc::new(cfg.routes);
        let handle = tokio::spawn(listen_loop(socket, routes, tx));

        // Send the trap from a separate socket.
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sender
            .send_to(&coldstart_trap_v2c(), bound_addr)
            .await
            .unwrap();

        // Wait up to a second for the listener to emit.
        let received = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .expect("listener should emit within 1s")
            .expect("channel still open");

        match received {
            AgentMessage::CheckResult(r) => {
                assert_eq!(r.component_id, comp_id);
                assert!(matches!(r.check_type, CheckType::SnmpTrap));
                assert_eq!(r.exit_code, 2);
            }
            _ => panic!("expected CheckResult"),
        }

        handle.abort();
    }
}
