# CLAUDE.md - crates/cli (appctl)

## Purpose
Command-line tool for administrators and schedulers. Calls backend REST API. Returns structured exit codes for scheduler integration.

## Exit Codes (CRITICAL for scheduler integration)
- 0: Success
- 1: Failure (action failed)
- 2: Timeout (--wait exceeded)
- 3: Auth error
- 4: Not found
- 5: Permission denied

## Commands
```
appctl start <app-name> [--wait] [--timeout 300] [--dry-run]
appctl stop <app-name> [--wait] [--timeout 300]
appctl status <app-name> [--format json|table|short]
appctl switchover <app-name> --target-site <site> --mode <FULL|SELECTIVE|PROGRESSIVE> [--wait]
appctl diagnose <app-name> [--format json|table]
appctl rebuild <app-name> [--components <ids>] [--dry-run] [--wait]
```

## Configuration
```bash
APPCONTROL_URL=https://appcontrol.company.com
APPCONTROL_API_KEY=ac_xxxxxxxxxxxx
```

## Dependencies
clap 4 (derive), reqwest, serde_json, tokio, appcontrol-common
