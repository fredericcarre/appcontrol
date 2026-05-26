---
title: Example maps & ingestion patterns
description: Concrete examples for every ingestion path — JSON, CSV, Git roundtrip — with knowledge maturity declared by the source.
---

# Example maps &amp; ingestion patterns

These examples are designed to be **copy-pasted directly** into a
`curl` command or the in-app *Captation* wizard. They walk through the
methodology with concrete data, so you can see what AppControl
ingests, how it stores it, and how the maturity ladder is enforced.

Every example follows the same principle: **the source declares its
maturity, AppControl never imposes one**. A scrape stays
`candidate`, a Git-reviewed export arrives `reviewed`, a manual
creation keeps `draft`.

---

## 1. A minimal three-tier web application — JSON v4

A classic load-balancer → web → database trio, suitable as the
*first map* a team creates during the onboarding workshop. We let
the database default apply (`draft`) so the team sees the components
arrive in the review backlog with the amber pip.

```bash
curl -X POST https://appcontrol/api/v1/import/json \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d @- <<'JSON'
{
  "format_version": "4.0",
  "application": {
    "name": "Acme Webshop",
    "description": "Public e-commerce front",
    "tags": ["public", "tier-1"],
    "components": [
      {
        "name": "haproxy-front",
        "component_type": "loadbalancer",
        "host": "lb-01.prod",
        "commands": {
          "check": "systemctl is-active haproxy",
          "start": "systemctl start haproxy",
          "stop":  "systemctl stop haproxy"
        }
      },
      {
        "name": "webshop-app",
        "component_type": "appserver",
        "host": "app-01.prod",
        "commands": {
          "check": "curl -fsS http://localhost:8080/actuator/health",
          "start": "systemctl start webshop",
          "stop":  "systemctl stop webshop"
        }
      },
      {
        "name": "webshop-db",
        "component_type": "database",
        "host": "db-01.prod",
        "commands": {
          "check": "pg_isready -h localhost",
          "start": "systemctl start postgresql",
          "stop":  "systemctl stop postgresql"
        }
      }
    ],
    "dependencies": [
      { "from": "haproxy-front", "to": "webshop-app" },
      { "from": "webshop-app",   "to": "webshop-db" }
    ]
  }
}
JSON
```

What happens after the call:

- Three components are created with `knowledge_status = 'draft'` and
  `confidence_score = 0.5` (the DB default — nothing was declared).
- They appear in the **map view** with **amber pips** in the top-left
  corner, telling the reviewer they're freshly created.
- The activation level of the application starts at the default `4`
  (direct ops). The team can drop it to `1` (advisory) immediately
  if they want a safety period.

<!-- SCREENSHOT:example-webshop-map -->

---

## 2. A Git-reviewed map declaring maturity

The team owns its maps in a Git repository. A CI pipeline runs on
every merge and pushes the JSON through the import API. The pipeline
declares the maturity it inherits from the Git review:

```bash
curl -X POST https://appcontrol/api/v1/import/json \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "json": "<the same JSON as above, serialised>",
    "default_knowledge_status": "reviewed",
    "default_confidence_score": 0.85
  }'
```

What changes:

- The three components arrive at **`reviewed`** because the CI
  declared it. On the map, **no pip is shown** for these components —
  they're flagged as ready.
- A reviewer can still bump individual components to `validated`
  through the UI after a successful drill.

<!-- SCREENSHOT:example-reviewed-map -->

---

## 3. A mixed map — per-component overrides

Sometimes a single import contains components of varying maturity. The
JSON v4 schema accepts a `knowledge_status` field per component:

```json
{
  "application": {
    "name": "Billing Core",
    "components": [
      {
        "name": "billing-api",
        "knowledge_status": "validated",
        "confidence_score": 0.95,
        "component_type": "service",
        "host": "srv-12.prod"
      },
      {
        "name": "billing-db",
        "knowledge_status": "reviewed",
        "component_type": "database",
        "host": "srv-13.prod"
      },
      {
        "name": "new-experimental-cache",
        "component_type": "cache",
        "host": "srv-14.prod"
      }
    ]
  },
  "default_knowledge_status": "draft"
}
```

Resolution:

| Component | Declared in row | Declared in request | Final status |
|---|---|---|---|
| `billing-api` | `validated` | `draft` | `validated` |
| `billing-db` | `reviewed` | `draft` | `reviewed` |
| `new-experimental-cache` | (none) | `draft` | `draft` |

Row override wins over request default; request default wins over
DB default. The order is documented in the methodology, § 4.5.

---

## 4. CMDB scrape — JSON with maturity declared as `candidate`

A daily cron pulls from ServiceNow and pushes the result. It's
honest about the data being raw:

```bash
curl -X POST https://appcontrol/api/v1/ingestion/cmdb \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "application_id": "<billing-core-uuid>",
    "source": "servicenow-nightly",
    "default_knowledge_status": "candidate",
    "components": [
      { "name": "billing-api", "component_type": "service", "host": "srv-12.prod",
        "tags": ["java", "owner:billing"] },
      { "name": "billing-worker", "component_type": "batch", "host": "srv-12.prod",
        "tags": ["java"] }
    ]
  }'
```

Result: the components are upserted. The components that already
existed keep their previous knowledge_status (e.g. `validated` from a
previous review) — `default_knowledge_status` only OVERWRITES via
the post-ingest sweep, but the row override logic on import doesn't
apply here. (In practice, an ingest with maturity `candidate` *will*
downgrade reviewed components — this is intentional because the
caller is declaring a raw scrape over the whole set.)

> :material-information: **Note** — to avoid downgrading reviewed
> components, run the ingest without `default_knowledge_status` and
> let the existing values stay in place. The DB default only applies
> to newly created rows.

---

## 5. CSV upload via the Captation wizard

The same data, but as a CSV pushed via the UI wizard or `curl`:

```csv
name,component_type,host,tags
billing-api,service,srv-12.prod,java;owner:billing
billing-worker,batch,srv-12.prod,java
```

```bash
curl -X POST 'https://appcontrol/api/v1/ingestion/cmdb/csv?application_id=<uuid>&knowledge_status=candidate' \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: text/csv" \
  --data-binary @components.csv
```

The wizard in the Captation page does exactly this — pick the source,
the format, the maturity, paste the CSV, hit *Ingérer*.

<!-- SCREENSHOT:captation-wizard -->

---

## 6. Network flow referential — CSV

Lists authorised traffic between components. AppControl resolves the
endpoints against existing components by name OR `host:port`:

```csv
from,to,port,protocol
billing-api,billing-db,5432,tcp
billing-worker,billing-db,5432,tcp
billing-api,auth-service,443,tcp
```

```bash
curl -X POST 'https://appcontrol/api/v1/ingestion/flows/csv?application_id=<uuid>&knowledge_status=reviewed' \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: text/csv" \
  --data-binary @flows.csv
```

Each matched flow becomes a dependency edge. Unmatched endpoints
are listed in the response report but don't abort the run.

---

## 7. ITSM incidents — JSON pull from ServiceNow

The backend fetches incidents itself, no need to export and re-upload:

```bash
curl -X POST https://appcontrol/api/v1/ingestion/pull/servicenow \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "organization_id": "<org-uuid>",
    "application_id": "<billing-core-uuid>",
    "instance_url": "https://acme.service-now.com",
    "auth_token_env_var": "SERVICENOW_AUTH_B64",
    "auth_scheme": "basic",
    "query": "priority<=2^opened_at>=javascript:gs.daysAgoStart(7)",
    "limit": 200
  }'
```

The token never leaves the backend — `SERVICENOW_AUTH_B64` is read
from the backend's environment.

<!-- SCREENSHOT:incidents-pull -->

---

## 8. Round-tripping a map through Git

Push the current map of an app to a configured Git remote:

```bash
curl -X POST https://appcontrol/api/v1/apps/<app-uuid>/git/push \
  -H "Authorization: Bearer $TOKEN"
```

The exported JSON carries **per-component knowledge_status and
confidence_score**, so re-importing the same file preserves the
review maturity:

```json
{
  "schema_version": 1,
  "exported_at": "2026-05-25T10:14:00Z",
  "application": { "id": "...", "name": "Billing Core" },
  "components": [
    {
      "id": "...",
      "name": "billing-api",
      "knowledge_status": "validated",
      "confidence_score": 0.95
    }
  ],
  "dependencies": [
    {
      "from_component_id": "...",
      "to_component_id": "...",
      "knowledge_status": "validated",
      "confidence_score": 0.9
    }
  ]
}
```

When the same JSON is re-imported (or imported into a fresh AppControl
instance), each component arrives back at `validated` — **no work
lost, no manual re-review**.

---

## 9. Trying it now without writing curl

Open AppControl, go to **Captation** in the sidebar, scroll to the
*Wizard d'ingestion* section. The wizard offers exactly the same
sources, the same formats, and a *Maturité déclarée par la source*
selector with the five levels described in this page.

<!-- SCREENSHOT:captation-page -->

---

## Where each example fits in the methodology

| Example | Methodology phase | What it illustrates |
|---|---|---|
| 1. Minimal web app JSON | Phase 1 — Collecte / Phase 3 — Construction | First map, default DB maturity |
| 2. Git-reviewed JSON | Phase 3 § 4.5 — Modifications GitOps | Source declares maturity |
| 3. Mixed map | Phase 3 § 4.5 — Resolution order | Row override beats request default |
| 4. CMDB scrape | Phase 1 — Captation automatique | Honest candidate flagging |
| 5. CSV wizard | Phase 1 — Captation manuelle | UI-driven ingestion |
| 6. Flow referential | Phase 1 — Captation flux | Dependency resolution |
| 7. ITSM pull | Phase 5 — Apprentissage par incidents | Incident ingestion |
| 8. Git roundtrip | Phase 3 § 4.5 — GitOps roundtrip | Maturity preserved through Git |
| 9. UI wizard | Phase 1 — Captation manuelle | No-curl path |
