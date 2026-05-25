# SNMP Monitoring

AppControl agents can poll managed devices via **SNMPv2c GET** to drive the
state machine of components that don't run a process the agent can directly
observe — switches, routers, firewalls, UPS, PDUs, storage arrays,
mainframes, printers, and other network-attached gear that exposes an SNMP
interface.

This rounds out the agent's check repertoire: shell commands for hosts you
control, HTTP / TCP probes for application endpoints, and now SNMP for the
physical / network substrate underneath.

## When to use SNMP checks

| Situation | Recommended check type |
|---|---|
| Linux/Windows host you can install an agent on | `check_cmd` shell, or [native commands](METRICS.md) |
| HTTP/REST endpoint health | `NativeCommand::Http` |
| TCP port reachability | `NativeCommand::TcpConnect` |
| Switch port status, router uptime, firewall sessions, UPS battery | `NativeCommand::Snmp` |
| Storage array LUN status, fabric switch ports | `NativeCommand::Snmp` |
| Legacy systems (mainframe, midrange) with SNMP MIB | `NativeCommand::Snmp` |

## Configuration

An SNMP check is a typed `NativeCommand::Snmp` payload attached to a
component's `check_native` field. The same payload format applies for
fan-out cluster members.

```json
{
  "name": "Core-Switch-DC1",
  "component_type": "network",
  "hostname": "switch01.dc1.example.com",
  "check_native": {
    "kind": "snmp",
    "host": "switch01.dc1.example.com",
    "port": 161,
    "community": "public-ro",
    "oid": "1.3.6.1.2.1.2.2.1.8.3",
    "expect": { "op": "equals", "value": "1" },
    "timeout_seconds": 5
  }
}
```

| Field | Required | Default | Description |
|---|---|---|---|
| `host` | yes | — | DNS name or IP of the device |
| `port` | no | `161` | UDP port (rarely overridden) |
| `community` | yes | — | SNMPv2c community string (sent in cleartext — use only on trusted management networks) |
| `oid` | yes | — | Dotted-decimal OID. Symbolic MIB names are **not** resolved — translate them to numeric arcs before configuring |
| `expect` | no | `{"op":"present"}` | Expectation applied to the returned value |
| `timeout_seconds` | no | `5` | Per-request timeout (bind + GET share this budget) |

## Expectations (`expect`)

Each expectation produces a binary verdict (pass = exit 0, fail = exit 1)
which the FSM consumes exactly like any other check result.

### `present` (default)

```json
{ "op": "present" }
```

Succeeds as soon as the OID resolves to any value. Fails on
`noSuchObject`, `noSuchInstance`, timeout, or network error. Use when you
just want to confirm the device is alive and answering SNMP.

### `equals`

```json
{ "op": "equals", "value": "1" }
```

Stringifies the returned value and compares it to `value` (case-sensitive,
exact). Classic use: interface `ifOperStatus` (1 = up, 2 = down).

### `in_range`

```json
{ "op": "in_range", "min": 0, "max": 70 }
```

Numeric values only (Integer, Counter32/64, Unsigned32, Timeticks, Boolean).
Verdict passes when `min ≤ value ≤ max` inclusive. Useful for thresholds:
CPU temperature, free memory, battery charge percent.

### `regex`

```json
{ "op": "regex", "pattern": ".*prod.*" }
```

Treats the value as a UTF-8 string and matches against the regex. Useful
for `sysName.0`, descriptions, or any string identifier.

## Metrics extraction

Every successful SNMP GET emits a JSON object on stdout:

```json
{
  "oid": "1.3.6.1.2.1.2.2.1.8.3",
  "type": "Integer",
  "value": "1",
  "numeric_value": 1.0
}
```

This payload flows through the standard
[metrics extraction pipeline](METRICS.md) and lands in
`check_events.metrics`. Alert policies and dashboards can therefore
reference `metrics.numeric_value` or `metrics.value` directly — no
separate plumbing for SNMP-sourced signals.

## Security

* **Community strings are credentials.** Treat them like passwords.
  AppControl redacts them in Debug output, audit logs, and action_log
  details, but they travel in cleartext UDP packets between the agent and
  the device — keep SNMP traffic on a trusted management VLAN.
* **Read-only is enough.** AppControl never issues SNMP SET operations.
  Configure your devices with a read-only community for the AppControl
  agent.
* **SNMPv3 (authPriv) is on the roadmap.** v2c is sufficient on
  management networks but should never traverse hostile segments. A
  follow-up release will add v3 with SHA + AES so the same probe schema
  works against hostile networks.

## Common OIDs reference

| OID | Object | Type | Notes |
|---|---|---|---|
| `1.3.6.1.2.1.1.3.0` | `sysUpTime.0` | Timeticks | Device uptime in 1/100s; use `in_range` to detect reboots |
| `1.3.6.1.2.1.1.5.0` | `sysName.0` | OctetString | Hostname; use `regex` |
| `1.3.6.1.2.1.2.2.1.8.<N>` | `ifOperStatus.<N>` | Integer | Interface N: 1=up, 2=down, 3=testing, ... |
| `1.3.6.1.2.1.2.2.1.10.<N>` | `ifInOctets.<N>` | Counter32 | Bytes received on interface N |
| `1.3.6.1.2.1.25.3.3.1.2.<N>` | `hrProcessorLoad.<N>` | Integer | CPU N load percent (Host Resources MIB) |
| `1.3.6.1.4.1.318.1.1.1.2.2.1.0` | APC UPS battery capacity | Integer | Vendor-specific; check the device's MIB |

## Testing your configuration

Run a one-shot SNMP probe with `snmpget` from the agent host to validate
the OID and community before wiring it into a component:

```bash
snmpget -v 2c -c public-ro switch01.dc1.example.com 1.3.6.1.2.1.2.2.1.8.3
```

If `snmpget` returns the expected value, AppControl will too.

## Trap receiver (push)

In addition to **polling** OIDs (pull), agents can **receive** SNMPv1/v2c
traps (push) and turn them into AppControl events. Devices that emit
traps for state changes — switches signalling link-down, UPS signalling
battery alarms, storage arrays signalling LUN faults — get translated
into FSM transitions on the configured component.

### Enabling

```yaml
# agent.yaml
snmp_traps:
  enabled: true
  listen_addr: "0.0.0.0:1162"   # port 162 is privileged; 1162 is the standard non-priv alternative
  routes:
    - name: "Cisco link-down on core switch"
      source_host: "10.1.0.10"          # or "*" for any source
      oid_prefix: "1.3.6.1.6.3.1.1.5.3" # linkDown trap (RFC 1907)
      component_id: "<uuid-of-Core-Switch-DC1-in-AppControl>"
      exit_code: 2                       # 0=Running, 1=Degraded, 2=Failed
    - name: "APC UPS power loss"
      source_host: "10.1.0.20"
      oid_prefix: "1.3.6.1.4.1.318.0.5"
      component_id: "<uuid-of-UPS-Battery>"
      exit_code: 1                       # warning, not full failure
```

Routes are evaluated top-to-bottom; the first match wins. `source_host =
"*"` matches any source, `oid_prefix = ""` matches any trap OID.

### What the agent does on receipt

1. Parses the UDP datagram with `snmp2::Pdu::from_bytes`.
2. Skips anything that isn't a trap (GetRequest, Response, ...).
3. Extracts the trap OID:
   * v2c — value of `snmpTrapOID.0` (1.3.6.1.6.3.1.1.4.1.0) in varbinds
   * v1 — composed from `enterprise` + `generic_trap` + `specific_trap`
4. Matches against `routes`. If none matches, the trap is logged at
   debug and discarded.
5. Synthesises a `CheckResult` with `check_type=SnmpTrap`, the route's
   `exit_code`, and stdout containing the JSON:

   ```json
   {
     "source_ip": "10.1.0.10",
     "trap_oid":  "1.3.6.1.6.3.1.1.5.3",
     "route":     "Cisco link-down on core switch",
     "varbinds":  [
       { "oid": "1.3.6.1.2.1.1.3.0",     "value": "12345678" },
       { "oid": "1.3.6.1.6.3.1.1.4.1.0", "value": "1.3.6.1.6.3.1.1.5.3" },
       { "oid": "1.3.6.1.2.1.2.2.1.1.3", "value": "3" }
     ]
   }
   ```

6. Forwards it on the same gateway WebSocket as native checks. The
   backend FSM treats it identically to a failing health check — same
   `state_transitions` row, same alerting policies fire, same
   `check_events.metrics` extraction.

### Security notes

* **Community strings are not validated server-side.** Anyone who can
  reach the listener port and knows your OID layout can inject a fake
  trap. **Always firewall the listener port to your real device VLAN.**
  v3 (authPriv) is planned for the follow-up.
* **Bind interface:** prefer `10.x.x.x:1162` or `127.0.0.1:1162` over
  `0.0.0.0:1162` if the agent host has multiple NICs. The example
  defaults to `0.0.0.0` to keep first-run friction low.
* **Port 162 requires root on Unix.** If you must use the standard
  port, run the agent as a user with `CAP_NET_BIND_SERVICE` (Linux),
  or NAT 162 → 1162 at the firewall.

### Testing the receiver

```bash
# Send a synthetic coldStart trap from another host:
snmptrap -v 2c -c public <agent-host>:1162 '' \
    1.3.6.1.6.3.1.1.5.1
# Watch the agent log:
journalctl -u appcontrol-agent -f
```

## Limitations (current)

* **v2c only** — v3 (authPriv) lands in a follow-up release for both
  poll and trap paths.
* **One OID per poll check** — bulk GET / walk is planned but not yet
  exposed.
* **No symbolic MIB resolution** — supply numeric OIDs.
* **Trap source filter** — exact IP or `*` only; CIDR matching is
  planned.
