# Permissions & Access Control

## Overview

AppControl has two independent permission systems:

| System | What it controls | Assigned to |
|--------|-----------------|-------------|
| **Platform role** | Admin features (manage users, teams, sites, agents) | Each user |
| **App permission** | What a user can do on a specific application | Users or teams, per app |

## Platform Roles

Every user has one global platform role. This controls **admin features only**, not application access.

| Role | Can do |
|------|--------|
| `admin` | Everything. Sees all apps (implicit owner). Manages users, teams, sites, agents. |
| `operator` | Can operate apps they have permission on. Cannot manage users or teams. |
| `editor` | Same as operator but intended for users who edit app configurations. |
| `viewer` | Read-only platform access. Can still operate apps if granted app-level permission. |

**The platform role does NOT restrict app permissions.** A `viewer` with `operate` permission on an app can start/stop that app.

**Recommendation:** Use `operator` for all regular users. The platform role only matters for admin features.

## App Permissions

App permissions control what a user can do on a **specific application**. They are assigned to **users** or **teams** per app.

### Permission Levels (lowest to highest)

| Level | Can do |
|-------|--------|
| `view` | See the application map, component states, and history |
| `operate` | Start, stop, restart components and the full application |
| `edit` | Modify components, dependencies, groups, configuration |
| `manage` | Change permissions, share the app with other users/teams |
| `owner` | Delete the application. Full control. |

### How Permissions Are Resolved

When a user accesses an app, their **effective permission** is calculated:

```
effective = MAX(
    direct user permission on this app,
    permission from team A on this app,
    permission from team B on this app,
    ...
)
```

- If a user has `operate` directly AND their team has `manage`, the effective level is `manage`.
- Admin users always have implicit `owner` on all apps.
- **No permission = app is invisible** on the dashboard.

### Permission Expiry

Permissions can have an optional `expires_at` date. Expired permissions are ignored in the effective calculation.

## Users, Teams, and Apps

### The Model

```
                  Admin (sees everything)
                       |
    +-----------+------+------+-----------+
    |           |             |           |
  User A     User B        User C     User D
    |           |             |
    +---Team Prod---+    Team DBA
    |               |         |
    |               |         |
    v               v         v
 App "Banking"   App "CRM"  App "Banking"
   (operate)      (view)      (manage)
```

### Visibility Rules

| User type | What they see on the dashboard |
|-----------|-------------------------------|
| Admin | All applications |
| Non-admin with permissions | Only apps where they have at least `view` (direct or via team) |
| Non-admin without permissions | Empty dashboard |

### Workflow

1. **Admin creates users** (Users page)
2. **Admin creates teams** (Teams page)
3. **Admin adds users to teams** (Team detail dialog)
4. **Admin shares apps with teams** (Share button on map view, or `appcontrol.sh`)

## Provisioning with appcontrol.sh

For automated setup, use the provisioning script:

```bash
# Individual commands
./appcontrol.sh create-user --email alice@corp.com --name "Alice" --password changeme
./appcontrol.sh create-team --name "Equipe Prod"
./appcontrol.sh add-member --team "Equipe Prod" --email alice@corp.com
./appcontrol.sh grant-access --app "Core Banking" --team "Equipe Prod" --level operate

# Or bulk provision from a JSON file
./appcontrol.sh provision --file setup.json
```

### setup.json Format

```json
{
  "users": [
    { "email": "alice@corp.com", "name": "Alice", "role": "operator", "password": "changeme" },
    { "email": "bob@corp.com",   "name": "Bob",   "role": "operator", "password": "changeme" }
  ],
  "teams": [
    {
      "name": "Equipe Prod",
      "description": "Production operations",
      "members": ["alice@corp.com", "bob@corp.com"]
    }
  ],
  "permissions": [
    { "app": "Core Banking", "team": "Equipe Prod", "level": "operate" },
    { "app": "Data Pipeline", "team": "Equipe Prod", "level": "view" }
  ]
}
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `APPCONTROL_URL` | `http://localhost:3001` | Backend API URL |
| `APPCONTROL_EMAIL` | `admin@localhost` | Admin login email |
| `APPCONTROL_PASSWORD` | `admin` | Admin login password |

## UI Quick Reference

| Action | Where |
|--------|-------|
| Create users | Users page (admin only) |
| Create teams | Teams page |
| Add users to teams | Teams page > click team > Add member |
| Share app with team/user | Map view > Share button > User/Team toggle |
| See who has access to an app | Map view > Share button > permission list |
