# CLAUDE.md - frontend/

## Purpose
React 18 SPA for AppControl. Dashboard, interactive maps (React Flow), real-time updates (WebSocket), permissions/sharing, command execution, diagnostic views, supervision mode.

## Tech Stack
- React 18, TypeScript 5.3+, Vite 5
- Tailwind CSS 3.4, shadcn/ui
- @xyflow/react 12+ (React Flow) for DAG maps
- @tanstack/react-query 5 for server state
- zustand 4 for client state
- lucide-react for icons

## Structure
```
frontend/src/
├── main.tsx
├── App.tsx                          # Root: router + layout
├── api/
│   ├── client.ts                    # Axios + JWT interceptor + refresh
│   ├── apps.ts                      # useApps, useApp, useStartApp, useStopApp, useDiagnose...
│   ├── components.ts                # useComponents, useStartComponent, useExecuteCommand...
│   ├── teams.ts                     # useTeams, useCreateTeam, useAddMember...
│   ├── permissions.ts               # usePermissions, useGrantPermission, useShareLink...
│   └── reports.ts                   # useAvailabilityReport, useIncidentsReport...
├── stores/
│   ├── auth.ts                      # User, JWT, login/logout
│   ├── ui.ts                        # Sidebar collapsed, dark mode, active view
│   └── websocket.ts                 # WS connection, subscriptions, reconnect
├── hooks/
│   ├── use-websocket.ts             # Connect, subscribe to app, receive events
│   ├── use-permission.ts            # useEffectivePermission(appId) → PermissionLevel
│   └── use-keyboard.ts             # Global keyboard shortcuts
├── components/
│   ├── ui/                          # shadcn/ui components
│   ├── layout/
│   │   ├── Sidebar.tsx              # 60px collapsed / 240px expanded
│   │   ├── Breadcrumb.tsx
│   │   └── Header.tsx
│   ├── maps/
│   │   ├── ComponentNode.tsx        # Custom React Flow node
│   │   ├── AppMap.tsx               # React Flow canvas + edges
│   │   ├── MapToolbar.tsx           # Actions: Start All, Stop, Branch, Switchover, Diagnose, Export
│   │   ├── DetailPanel.tsx          # Right panel: state, checks, history, config tabs
│   │   └── DiagnosticPanel.tsx      # Diagnostic results with recommendations
│   ├── share/
│   │   ├── ShareModal.tsx           # Google Docs-style sharing dialog
│   │   ├── PermissionRow.tsx        # User/team row with level selector
│   │   └── ShareLinkRow.tsx         # Share link with copy button
│   ├── commands/
│   │   ├── CommandModal.tsx         # Execute command with terminal output
│   │   └── TerminalOutput.tsx       # Monospace scrollable output
│   ├── teams/
│   │   ├── TeamList.tsx
│   │   └── InviteModal.tsx
│   ├── onboarding/
│   │   ├── WelcomeWizard.tsx        # 7-step first-use wizard
│   │   └── AppCreationWizard.tsx    # 4-step app creation with drag & drop
│   └── supervision/
│       └── SupervisionMode.tsx      # Full-screen NOC mode, rotating views
├── pages/
│   ├── DashboardPage.tsx
│   ├── MapViewPage.tsx
│   ├── TeamsPage.tsx
│   ├── AgentsPage.tsx
│   ├── ReportsPage.tsx
│   ├── SettingsPage.tsx
│   ├── OnboardingPage.tsx
│   ├── ShareLinkPage.tsx              # Public share link viewer (read-only map access)
│   ├── ApiKeysPage.tsx                # API key management for scheduler integration
│   └── ImportPage.tsx                 # Application import (XML/JSON/YAML upload)
└── lib/
    ├── colors.ts                    # State color palette
    └── permissions.ts              # Permission level helpers
```

## Component State Colors (EXACT — do not change)
| State | Background | Border | Animation |
|-------|-----------|--------|-----------|
| RUNNING | #E8F5E9 | #4CAF50 | none |
| DEGRADED | #FFF3E0 | #FF9800 | none |
| FAILED | #FFEBEE | #F44336 | none |
| STOPPED | #F5F5F5 | #9E9E9E | none |
| STARTING | #E3F2FD | #2196F3 | pulse 1.5s |
| STOPPING | #E3F2FD | #2196F3 | pulse 1.5s |
| UNREACHABLE | rgba(33,33,33,0.1) | #212121 | none |
| UNKNOWN | #FFFFFF | #BDBDBD (dashed) | none |
| Error branch | #FFE0E6 | #FF6B8A | none (edges pulse) |

## Component Type Icons (lucide-react)
Component types are flexible strings - any value is allowed. Common types are mapped to icons:

| Type | Icon | Color |
|------|------|-------|
| database, db | Database | #1565C0 |
| middleware, mq, queue | Layers | #6A1B9A |
| appserver, application, server | Server | #2E7D32 |
| webfront, webserver, frontend | Globe | #E65100 |
| service, api | Cog | #37474F |
| batch, job | Clock | #4E342E |
| loadbalancer, proxy, gateway | Network | #0277BD |
| cache, redis | Zap | #F57C00 |
| (other/unknown) | Box | #455A64 |

## Keyboard Shortcuts
| Key | Action | Context |
|-----|--------|---------|
| Ctrl+F | Search component | Map |
| Ctrl+A | Select all | Map |
| Delete | Delete selected | Map (edit mode) |
| Space | Toggle start/stop | Map (operate) |
| F5 | Refresh checks | Map |
| F11 | Supervision mode | Anywhere |
| Ctrl+E | Toggle detail panel | Map |
| Ctrl+P | Export map | Map |
| ? | Show shortcuts help | Anywhere |

## WebSocket Protocol
```typescript
// Connect
const ws = new WebSocket(`wss://backend/ws?token=${jwt}`);
// Subscribe
ws.send(JSON.stringify({ type: 'subscribe', payload: { appId: 'uuid' } }));
// Events received (filtered by permission):
// state_change, check_result, command_result, switchover_progress, agent_status, permission_change
```

## Accessibility (WCAG 2.1 AA)
- All actions reachable by keyboard (Tab navigation)
- Visible focus indicators
- aria-labels on all interactive elements
- State conveyed by shape + color (not color alone) — icons for each state
- Dark mode via Tailwind `dark:` classes
