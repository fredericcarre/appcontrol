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

## Limitations (current)

* **v2c only** — v3 (authPriv) lands in a follow-up release.
* **One OID per check** — bulk GET / walk is planned but not yet exposed.
* **No symbolic MIB resolution** — supply numeric OIDs.
* **No trap receiver** — push-based SNMP traps will be a separate
  agent feature with its own routing table.
