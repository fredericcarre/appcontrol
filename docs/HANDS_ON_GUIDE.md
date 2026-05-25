---
title: Hands-on guide — take AppControl for a test drive
description: Step-by-step walkthrough to test every methodology phase against a running AppControl stack, using ready-made example files.
---

# Hands-on guide

The fastest way to get an honest read on AppControl. You'll run a
stack locally, import a pre-made map, exercise every methodology
phase via the UI and the API, and end with a fully populated demo
you can show colleagues.

Estimated time: **30 minutes**, no Rust knowledge required.

---

## Prerequisites

You need either:

- **Docker compose** (recommended — `docker compose up` from the repo root)
- Or an existing AppControl install reachable on `http://localhost:3000`

Plus `curl` and `jq` (`apt-get install jq` on Debian/Ubuntu,
`brew install jq` on macOS).

---

## Step 0 — Start the stack and log in

```bash
git clone https://github.com/fredericcarre/appcontrol
cd appcontrol
docker compose -f docker/docker-compose.yaml up -d
```

Wait for the health check to pass:

```bash
until curl -fsS http://localhost:3000/health > /dev/null; do sleep 2; done
```

Open `http://localhost:8080` in a browser (or whatever port your
nginx fronts AppControl on) and log in with the seed credentials
documented in `docker/docker-compose.yaml`.

<!-- SCREENSHOT:login -->

---

## Step 1 — One-shot walkthrough (recommended first run)

The fastest way to see every methodology phase in motion:

```bash
BACKEND_URL=http://localhost:3000 \
  ADMIN_EMAIL=admin@localhost \
  ADMIN_PASSWORD=admin \
  ./scripts/methodology-walkthrough.sh
```

The script:

1. Imports `examples/methodology-demo.json` declaring `reviewed`
   maturity → the resulting map shows most components without a
   knowledge pip (validated/reviewed).
2. Adds two components from `examples/raw-cmdb-scrape.json` declared
   `candidate` → they appear with slate pips on the map.
3. Bumps the activation level: 4 → 1 (advisory) → 2 (diagnostic).
4. Drops a `todo` annotation on `webshop-cache` and promotes it
   `candidate` → `draft` (slate → amber pip).
5. Creates the **Spring Boot JDBC pattern** from
   `examples/pattern-spring-boot-jdbc.json` and propagates it to
   every matching component.
6. Prints the knowledge maturity summary for the app.
7. Pushes the map to a Git remote if one is configured (skipped
   silently otherwise).

Open the UI side-by-side and watch each step land in real time.

<!-- SCREENSHOT:dashboard -->

---

## Step 2 — Same things, one step at a time

Run the steps manually to feel each one.

### 2.1 Import the mature demo map

```bash
TOKEN=$(curl -sS -X POST http://localhost:3000/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@localhost","password":"admin"}' | jq -r '.access_token // .token')

curl -sS -X POST http://localhost:3000/api/v1/import/json \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "$(jq -n --arg json "$(cat examples/methodology-demo.json)" \
        '{json: $json,
          default_knowledge_status: "reviewed",
          default_confidence_score: 0.85}')"
```

Note the `application_id` in the response — used in every later call:

```bash
APP_ID="<paste-the-uuid-here>"
```

<!-- SCREENSHOT:example-webshop-map -->

### 2.2 Inspect the knowledge state from the map

Open `http://localhost:8080/apps/$APP_ID` and notice:

- `haproxy-front` and `webshop-db-primary` show **no pip** — they
  were declared `validated` in the JSON.
- `webshop-app-1` and `webshop-app-2` show **indigo pips** —
  declared `reviewed`.
- `webshop-cache` shows a **slate pip** — declared `candidate`.
- `webshop-worker` shows **no declared status** in the JSON, but
  the request default was `reviewed`, so it lands `reviewed`.

The **MaturityStrip** at the top of the page reports the validated
coverage % at a glance.

<!-- SCREENSHOT:knowledge-tab -->

### 2.3 Try the Captation wizard

Sidebar → **Captation**. Scroll to *Wizard d'ingestion*. Pick CMDB,
JSON, target the demo app, maturity = *candidate*, paste the body
from `examples/raw-cmdb-scrape.json` (after editing the
application_id), hit *Ingérer*.

You'll see the report panel light up green with `created: 2`. Back
on the map, two new components appear with slate pips.

<!-- SCREENSHOT:captation-wizard -->

### 2.4 Move the activation ladder

Sidebar → click the app → header **MaturityStrip** → click the
activation badge. The Activation page lets you move through the 5
levels with one click each.

Try: 4 → 1 (advisory). Then try to start the app from the
MapToolbar — refused with a 403, because operations require level 3
or above.

<!-- SCREENSHOT:activation-page -->

### 2.5 Add an annotation, promote a component

Click on `webshop-cache` in the map → **Knowledge** tab. You'll see:

- The promotion ladder with `candidate` highlighted (its current state)
- A confidence slider at 30 %
- The annotations panel below

Click **draft** to promote. The slate pip on the map flips to amber.

Add a TODO annotation: "Pas de health check pour l'instant. À
raffiner après la revue Redis." Resolve it later from the same panel.

### 2.6 Create a pattern, propagate

```bash
curl -sS -X POST http://localhost:3000/api/v1/patterns \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d @examples/pattern-spring-boot-jdbc.json

PATTERN_ID="<paste-id>"
curl -sS http://localhost:3000/api/v1/patterns/$PATTERN_ID/candidates \
  -H "Authorization: Bearer $TOKEN" | jq

# Propagate to all candidates:
CANDIDATES=$(curl -sS http://localhost:3000/api/v1/patterns/$PATTERN_ID/candidates \
  -H "Authorization: Bearer $TOKEN" | jq '[.candidates[].component_id]')

curl -sS -X POST http://localhost:3000/api/v1/patterns/$PATTERN_ID/propagate \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"component_ids\": $CANDIDATES}"
```

In the UI: sidebar → **Captation** → scroll to *Bibliothèque de
patterns*. Click the new pattern card. The candidate list opens —
select all and click *Appliquer*.

<!-- SCREENSHOT:pattern-propagation -->

### 2.7 Read the knowledge summary

```bash
curl -sS http://localhost:3000/api/v1/apps/$APP_ID/knowledge/summary \
  -H "Authorization: Bearer $TOKEN" | jq
```

Returns the per-status counts plus the headline `validated_coverage`
ratio — the metric you'd put on the COMEX dashboard.

---

## Step 3 — Bring your own data

When you're done playing with the demo, try the same flow with your
own data:

- **Got a YAML/JSON map already?** Drop it in via the **Import** page
  in the sidebar, declaring whatever maturity matches its origin.
- **Got a CMDB export?** Use the **Captation wizard** with CSV or
  JSON, declaring `candidate`.
- **Got a Git repo full of YAML maps?** Configure a Git remote
  (`POST /api/v1/git/remotes` as admin), bind your app to it
  (`PUT /api/v1/apps/:id/git`), then push and roundtrip via
  `POST /apps/:id/git/push`.

---

## Step 4 — What to look at next

| If you want… | Read |
|---|---|
| The strategic narrative | (draft) docs/strategy.html |
| The full methodology | [Methodology in 9 screens](METHODOLOGY_WALKTHROUGH.md) |
| Concrete ingestion examples | [Example maps](EXAMPLE_MAPS.md) |
| The pricing model + ROI simulator | (draft) docs/pricing.html |
| What's in the code right now | [Implementation status](implementation-status.md) — kept off the live site while v0.9 |

---

## Cleaning up

```bash
docker compose -f docker/docker-compose.yaml down -v
```

Removes the containers and the persistent volumes — fresh slate
next time.
