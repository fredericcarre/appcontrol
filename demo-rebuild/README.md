# Demo scenario — Critical application rebuild & DR switchover

This directory ships a ready-to-run demonstration of AppControl's **fan-out cluster mode**, **surgical rebuild** and **DR switchover** against a realistic banking-style application: a web portal, a fan-out JBoss tier, an Oracle RAC database and an MQ bus, with a primary site and a DR site.

The rebuild commands are wired to mock **XL Deploy** and **XL Release** CLIs so you can narrate the "AppControl is a control plane on top of your existing deployment tooling" angle without needing the real products installed.

---

## What the audience will see

1. **A business-shaped map** imported from git (GitOps-style ops).
2. **Start, live state** of a multi-tier app with a 6-node fan-out tier.
3. **Surgical rebuild of a single degraded member** via mock XL Deploy — compared against the usual "redeploy everything" reflex.
4. **Protected rebuild** of a sensitive component (Oracle RAC) delegated to an XL Release pipeline with approval phase.
5. **Site outage + switchover** to the DR site: same map, one click, 6 audited phases.
6. **DORA-ready audit log** exported in seconds.

---

## Prerequisites

- AppControl 1.16+ running locally (backend + gateway + one agent registered).
- Windows or Linux workstation (scripts provided for both).
- Two sites in AppControl with site codes **`PRIMARY`** (or any code you pick for the primary) and **`DR`**. The import wizard will ask you which one is primary — point it at your environment's primary site code, then edit the map if your DR site code is different from `DR` (search/replace in the JSON before import).

> The map uses a single `agent-primary` for every component so the whole demo runs on one machine. In a real deployment each host has its own agent. Nothing else changes.

---

## Setup (5 minutes, once)

### Windows

```cmd
REM From the extracted demo-rebuild\scripts\windows folder:
setup.bat
```

This creates `%TEMP%\appcontrol-demo-rebuild\primary\`, `\dr\` and `\bin\`, and installs the mock `xldeploy-cli.bat` and `xlr-cli.bat` inside `\bin\`. The map's `rebuild_cmd` entries already point to that path.

### Linux / macOS

```bash
cd demo-rebuild/scripts/linux
bash setup.sh
```

Creates `/tmp/appcontrol-demo-rebuild/{primary,dr,bin}` and installs the mock shell CLIs.

### Import the map

1. In AppControl, open **Import** → tab **Fetch from URL** (1.16+) or **Upload file**.
2. Pick `critical-banking-app-windows.json` (or `…-linux.json` for a Linux demo).
3. When the wizard prompts for the primary site, select your environment's primary site.
4. The map lands with 6 components, a fan-out JBoss tier with 6 members, and site overrides wired to the `DR` site code.

---

## Script de démo — 10 min chrono

### Opening hook (45 s)

> *"Before we plunge in: in your teams today, to operate a critical application you have a CMDB, a batch scheduler, a hypervision console, a supervision tool (Nagios or similar), a DR tooling, a DORA module and probably a ticketing system to stitch it all. Seven tools minimum. Seven licences. Seven teams that talk to each other poorly. Question to keep in mind during this demo: which of those are really indispensable tomorrow if one single tool does the full job end-to-end?"*

Leave it hanging. Start the demo.

### Act 1 — Git-sourced import (1 min)

- Open a terminal alongside the browser. Leave `watch-markers.bat` running in it (or `watch -n 2 ls ...` on Linux).
- In AppControl, import the map via **Fetch from URL** (if you have the repo published) or via file upload.
- As the map appears, point to its structure: F5 at the top, WebSphere Portal, JBoss App Tier (6-node fan-out badge visible), Oracle RAC and MQ at the bottom.

**Say:** *"This map lives in git, version-controlled, reviewed in PR. The operation of your application is now code. Not a Visio diagram on SharePoint, not a Word runbook — code."*

### Act 2 — Start the app, cascaded by the DAG (2 min)

- Click **Start app**.
- Watch the DAG cascade: Oracle RAC + MQ first (level 0), then JBoss members in parallel (the 6 light up as a wave), then WebSphere Portal, then F5.
- Side terminal: the markers appear one by one in `%TEMP%\appcontrol-demo-rebuild\primary\`.

**Say:** *"Nobody typed `systemctl start` anywhere. The startup order emerges from the dependency graph — JBoss starts in parallel because nothing blocks it, F5 waits for WebSphere's health check. This is **orchestration from the map**, not a 3000-line Python script in Control-M that nobody dares touch."*

### Act 3 — One node dies, surgical rebuild via XL Deploy (2 min)

In the side terminal:

```
corrupt-jboss-003.bat
```

In the map, member 003 turns red. Parent JBoss App Tier shows **RUNNING — 5/6 members healthy** (threshold is 80%, we're at 83%). No alert spam. The app keeps serving traffic.

Turn to the room: *"Usually, at 4 am, one of your 1200 JBoss falls. With your current stack — would you redeploy the whole pool? 20 minutes of downtime, a sleepy duty engineer, CAB approval at dawn. Watch what we do."*

Right-click member 003 → **Rebuild member**. The component's `rebuild_cmd` runs: `xldeploy-cli.bat deploy --package jboss-node:1.2.4 --target jboss-prd-003...`. The side terminal shows the XL Deploy invocation streaming.

3-4 seconds later the marker is back, the member returns to RUNNING, the parent is back to full-healthy.

**Say:** *"Surgical, not ballistic. **One node touched, no service interruption, audit trail preserved.** AppControl is a **client of XL Deploy**, not a replacement — we keep your deployment tooling, we just call it at the right moment with the right target."*

### Act 4 — Protected rebuild via XL Release + approval (1 min)

Now stop the Oracle RAC marker directly (or corrupt it in the terminal with `del %TEMP%\appcontrol-demo-rebuild\primary\oracle-rac.running`).

Oracle RAC turns red. Right-click → **Rebuild**.

AppControl sees `rebuild_protected: true` on Oracle RAC and routes to its dedicated `rebuild_cmd`: `xlr-cli.bat trigger --template "Oracle RAC Rebuild PROD"`. The terminal shows 4 tasks running, including a simulated approval.

**Say:** *"For high-risk components, the rebuild goes through the XL Release pipeline with CAB approval. Same product, two rebuild modes: surgical via XL Deploy for routine failures, formal via XL Release for critical ones. **Choice by policy, not by panic.**"*

### Act 5 — Site outage + switchover to DR (2 min)

In the terminal:

```
disaster-primary.bat
```

All primary-site markers disappear. Components cascade to FAILED within 10–30 seconds. The map goes red.

**Say:** *"Primary site is down — ransomware, datacenter, whatever. In your current world, how long is the DR procedure? 2 hours? 4 hours? How many people on the war-room call?"*

Click **Switchover** on the app → select site **DR** → run the 6-phase wizard:

1. **PREPARE** — readiness check on DR agents
2. **VALIDATE** — verifies each component has a `site_overrides[DR]` defined
3. **STOP_SOURCE** — graceful stop on primary (already down, skipped)
4. **SYNC** — sync hooks if defined
5. **START_TARGET** — runs each component's `start_cmd_override` on the DR site (markers appear in `\dr\`)
6. **COMMIT** — active site is now DR

The exact same map, now running on the DR markers. The side terminal visualizes markers moving from `primary\` to `dr\`.

**Say:** *"One map. Two sites. One click. Six audited phases. The DR plan is not a Word document any more — it's the map itself, versioned in git, executable. **DORA Article 12 wants you to prove annual DR tests with rollback. Your current team does it in how many days? Here: 5 minutes, exported as a signed PDF.**"*

### Act 6 — DORA audit export (30 s)

Open the app's **History** tab. All state transitions since the start of the demo are there — timestamped, with who/what/why. Click **Export audit** — a PDF/CSV comes out in 2 seconds.

**Say:** *"This table, immutable by architecture, is exactly what ACPR asks for under DORA since January 2025. Not a €40k add-on module — it's our first design rule."*

### Closing (45 s)

Leave the map on screen, the audit report open in another tab.

**Say:** *"What you saw in 10 minutes, today in your shop, mobilises: your CMDB to know what you have, your batch scheduler to orchestrate startup, your hypervision console to see state, your supervision tool to detect failures, your DR tool for site switchover, your audit module for DORA, your deployment tool (which you keep) for the actual bits, and your ticketing platform on top. **Minimum 7 tools, minimum 4 teams.** Here: one platform, one agent per machine, one screen."*

> *"I'm not asking you to kill anything tomorrow. I'm asking you to install AppControl **alongside** for 90 days. Import your existing config, run in shadow, let your own numbers tell you what stays and what goes."*

---

## Handling objections

**"XL Deploy already orchestrates deployment phases."**
→ *"XL Deploy orchestrates the **deployment** of a package. AppControl orchestrates the **lifecycle** of a running application. Deployment is 5 minutes a year. Operation is 525 600 minutes. Pick where to invest."*

**"We could script rebuild logic in XL Release."**
→ *"You could. But you'd also need to rebuild the FSM, the state transitions, the check-based detection, the DAG-aware sequencing, the site-aware overrides, the DORA audit. You'd end up reinventing AppControl inside XL Release, which was designed for something else. Been there, done it for you."*

**"Why check/start/stop when we have XL Deploy?"**
→ *"XL Deploy knows `installed / not installed`. AppControl needs to know `running / degraded / failed / starting / stopping / unreachable / stopped / unknown` — eight states with valid transitions between them. Without that, no SLO, no FSM, no real alerting, no meaningful audit. The four primitives are the atom of operations, not an incremental feature."*

**"Importing a 'DR map' — why?"**
→ **Don't** import one. The same map has `site_overrides` for each site. Switchover is one click inside AppControl — no second import, no second runbook, no second plan. If the demo audience asks, say *"one map, two sites, built-in"*.

**"What about total VM loss?"**
→ `rebuild_infra_cmd` with `rebuild_agent_id` (bastion) rebuilds the infrastructure before the app — part of the native model, not a bolt-on.

---

## Five phrases to plant, repeated by design

1. *"One map, one reality, one audit trail."*
2. *"Your DR plan becomes code."*
3. *"Orchestration emerges from the map."*
4. *"DORA audit isn't a module; it's the architecture."*
5. *"Install it alongside for 90 days — your own numbers will decide."*

---

## Reset between runs

```cmd
reset.bat      REM Windows
```

or

```bash
bash reset.sh  # Linux
```

Wipes markers + logs. AppControl's state transitions and audit log remain intact (that's intentional — it's part of what you show).

---

## Files in this directory

```
demo-rebuild/
├── README.md                            # You are here
├── critical-banking-app-windows.json    # Map to import (Windows markers + XL CLIs)
├── critical-banking-app-linux.json      # Map to import (Linux markers + XL CLIs)
└── scripts/
    ├── windows/
    │   ├── setup.bat               # Creates directories + installs mocks
    │   ├── xldeploy-cli.bat        # Mock XL Deploy CLI
    │   ├── xlr-cli.bat             # Mock XL Release CLI
    │   ├── corrupt-jboss-003.bat   # Simulate single-member failure
    │   ├── disaster-primary.bat    # Simulate primary-site outage
    │   ├── watch-markers.bat       # Live marker view (side terminal)
    │   └── reset.bat               # Clean state for re-run
    └── linux/
        ├── setup.sh
        ├── xldeploy-cli.sh
        ├── xlr-cli.sh
        ├── corrupt-jboss-003.sh
        ├── disaster-primary.sh
        └── reset.sh
```

All demo state (markers, mock CLI logs) lives under `%TEMP%\appcontrol-demo-rebuild\` (Windows) or `/tmp/appcontrol-demo-rebuild/` (Linux). Deleting that directory wipes the demo cleanly.
