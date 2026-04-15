#!/usr/bin/env bash
# =============================================================================
# appcontrol.sh — AppControl provisioning & management CLI
#
# Usage:
#   appcontrol.sh <command> [options]
#
# Commands:
#   create-user     Create a user account
#   create-team     Create a team
#   add-member      Add a user to a team
#   grant-access    Grant a team or user permission on an app
#   list-users      List all users
#   list-teams      List all teams
#   list-apps       List all applications
#   import-map      Import an application map (JSON/YAML)
#   provision       Bulk provision from a JSON file (users, teams, permissions)
#   init-catalog    Seed built-in component types into the catalog
#   import-catalog  Import custom component types from a JSON file
#   list-catalog    List all component types in the catalog
#
# Environment:
#   APPCONTROL_URL      Backend URL (default: http://localhost:3001)
#   APPCONTROL_EMAIL    Admin email (default: admin@localhost)
#   APPCONTROL_PASSWORD Admin password (default: admin)
# =============================================================================

set -euo pipefail

# ── Configuration ────────────────────────────────────────────────────────────
APPCONTROL_URL="${APPCONTROL_URL:-http://localhost:3001}"
APPCONTROL_EMAIL="${APPCONTROL_EMAIL:-admin@localhost}"
APPCONTROL_PASSWORD="${APPCONTROL_PASSWORD:-admin}"
API_BASE="${APPCONTROL_URL}/api/v1"
TOKEN=""

# ── Colors ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info()  { echo -e "${BLUE}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
die()   { error "$@"; exit 1; }

# ── HTTP Helpers ─────────────────────────────────────────────────────────────

login() {
  local resp
  resp=$(curl -sf -X POST "${API_BASE}/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"email\":\"${APPCONTROL_EMAIL}\",\"password\":\"${APPCONTROL_PASSWORD}\"}" 2>/dev/null) \
    || die "Login failed. Check APPCONTROL_URL, APPCONTROL_EMAIL, APPCONTROL_PASSWORD."
  TOKEN=$(echo "$resp" | jq -r '.token // .access_token // empty')
  [ -n "$TOKEN" ] || die "Login succeeded but no token returned."
}

api_get() {
  curl -sf -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" "$1" 2>/dev/null
}

api_post() {
  curl -sf -X POST -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" -d "$2" "$1" 2>/dev/null
}

api_delete() {
  curl -sf -X DELETE -H "Authorization: Bearer ${TOKEN}" -H "Content-Type: application/json" "$1" 2>/dev/null
}

ensure_login() {
  [ -n "$TOKEN" ] || login
}

# ── Lookup Helpers ───────────────────────────────────────────────────────────

# Find user ID by email. Returns empty string if not found.
find_user_id() {
  local email="$1"
  api_get "${API_BASE}/users" | jq -r ".users[] | select(.email == \"${email}\") | .id // empty" 2>/dev/null | head -1
}

# Find team ID by name. Returns empty string if not found.
find_team_id() {
  local name="$1"
  api_get "${API_BASE}/teams" | jq -r ".teams[] | select(.name == \"${name}\") | .id // empty" 2>/dev/null | head -1
}

# Find app ID by name. Returns empty string if not found.
find_app_id() {
  local name="$1"
  api_get "${API_BASE}/apps" | jq -r ".apps[] | select(.name == \"${name}\") | .id // empty" 2>/dev/null | head -1
}

# ── Commands ─────────────────────────────────────────────────────────────────

cmd_create_user() {
  local email="" name="" role="operator" password=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --email)    email="$2"; shift 2 ;;
      --name)     name="$2"; shift 2 ;;
      --role)     role="$2"; shift 2 ;;
      --password) password="$2"; shift 2 ;;
      *) die "Unknown option: $1. Usage: create-user --email EMAIL --name NAME [--role ROLE] [--password PWD]" ;;
    esac
  done
  [ -n "$email" ] || die "Missing --email"
  [ -n "$name" ] || die "Missing --name"
  ensure_login

  # Check if user already exists
  local existing_id
  existing_id=$(find_user_id "$email")
  if [ -n "$existing_id" ]; then
    warn "User '$email' already exists (id: ${existing_id})"
    echo "$existing_id"
    return 0
  fi

  local body="{\"email\":\"${email}\",\"display_name\":\"${name}\",\"role\":\"${role}\""
  [ -n "$password" ] && body+=",\"password\":\"${password}\""
  body+="}"

  local resp
  resp=$(api_post "${API_BASE}/users" "$body") || die "Failed to create user '$email'"
  local user_id
  user_id=$(echo "$resp" | jq -r '.id // empty')
  ok "User created: ${email} (role: ${role}, id: ${user_id})"
  echo "$user_id"
}

cmd_create_team() {
  local name="" description=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --name)        name="$2"; shift 2 ;;
      --description) description="$2"; shift 2 ;;
      *) die "Unknown option: $1. Usage: create-team --name NAME [--description DESC]" ;;
    esac
  done
  [ -n "$name" ] || die "Missing --name"
  ensure_login

  # Check if team already exists
  local existing_id
  existing_id=$(find_team_id "$name")
  if [ -n "$existing_id" ]; then
    warn "Team '$name' already exists (id: ${existing_id})"
    echo "$existing_id"
    return 0
  fi

  local body="{\"name\":\"${name}\",\"description\":\"${description}\"}"
  local resp
  resp=$(api_post "${API_BASE}/teams" "$body") || die "Failed to create team '$name'"
  local team_id
  team_id=$(echo "$resp" | jq -r '.id // empty')
  ok "Team created: ${name} (id: ${team_id})"
  echo "$team_id"
}

cmd_add_member() {
  local team="" email="" role="member"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --team)  team="$2"; shift 2 ;;
      --email) email="$2"; shift 2 ;;
      --role)  role="$2"; shift 2 ;;
      *) die "Unknown option: $1. Usage: add-member --team TEAM_NAME --email EMAIL [--role member|lead]" ;;
    esac
  done
  [ -n "$team" ] || die "Missing --team"
  [ -n "$email" ] || die "Missing --email"
  ensure_login

  local team_id user_id
  team_id=$(find_team_id "$team") || true
  [ -n "$team_id" ] || die "Team '${team}' not found"
  user_id=$(find_user_id "$email") || true
  [ -n "$user_id" ] || die "User '${email}' not found"

  local body="{\"user_id\":\"${user_id}\",\"role\":\"${role}\"}"
  api_post "${API_BASE}/teams/${team_id}/members" "$body" > /dev/null || warn "Failed to add '${email}' to '${team}' (may already be a member)"
  ok "Added ${email} to team ${team} (role: ${role})"
}

cmd_grant_access() {
  local app="" team="" user_email="" level="view"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --app)   app="$2"; shift 2 ;;
      --team)  team="$2"; shift 2 ;;
      --user)  user_email="$2"; shift 2 ;;
      --level) level="$2"; shift 2 ;;
      *) die "Unknown option: $1. Usage: grant-access --app APP_NAME (--team TEAM | --user EMAIL) --level LEVEL" ;;
    esac
  done
  [ -n "$app" ] || die "Missing --app"
  [ -n "$team" ] || [ -n "$user_email" ] || die "Missing --team or --user"
  ensure_login

  local app_id
  app_id=$(find_app_id "$app") || true
  [ -n "$app_id" ] || die "Application '${app}' not found"

  if [ -n "$team" ]; then
    local team_id
    team_id=$(find_team_id "$team") || true
    [ -n "$team_id" ] || die "Team '${team}' not found"
    local body="{\"team_id\":\"${team_id}\",\"permission_level\":\"${level}\"}"
    api_post "${API_BASE}/apps/${app_id}/permissions/teams" "$body" > /dev/null \
      || die "Failed to grant ${level} on '${app}' to team '${team}'"
    ok "Granted ${level} on '${app}' to team '${team}'"
  else
    local user_id
    user_id=$(find_user_id "$user_email") || true
    [ -n "$user_id" ] || die "User '${user_email}' not found"
    local body="{\"user_id\":\"${user_id}\",\"permission_level\":\"${level}\"}"
    api_post "${API_BASE}/apps/${app_id}/permissions/users" "$body" > /dev/null \
      || die "Failed to grant ${level} on '${app}' to user '${user_email}'"
    ok "Granted ${level} on '${app}' to user '${user_email}'"
  fi
}

cmd_list_users() {
  ensure_login
  api_get "${API_BASE}/users" | jq -r '.users[] | "\(.email)\t\(.display_name)\t\(.role)\t\(.is_active)"' 2>/dev/null \
    | column -t -s $'\t' -N "EMAIL,NAME,ROLE,ACTIVE"
}

cmd_list_teams() {
  ensure_login
  api_get "${API_BASE}/teams" | jq -r '.teams[] | "\(.name)\t\(.description // "-")\t\(.member_count // 0) members"' 2>/dev/null \
    | column -t -s $'\t' -N "NAME,DESCRIPTION,MEMBERS"
}

cmd_list_apps() {
  ensure_login
  api_get "${API_BASE}/apps" | jq -r '.apps[] | "\(.name)\t\(.global_state)\t\(.component_count) components"' 2>/dev/null \
    | column -t -s $'\t' -N "NAME,STATE,COMPONENTS"
}

cmd_import_map() {
  local file=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --file) file="$2"; shift 2 ;;
      *)      file="$1"; shift ;;
    esac
  done
  [ -n "$file" ] || die "Missing file. Usage: import-map --file MAP.json"
  [ -f "$file" ] || die "File not found: $file"
  ensure_login

  local resp
  resp=$(curl -sf -X POST -H "Authorization: Bearer ${TOKEN}" \
    -H "Content-Type: application/json" \
    -d @"$file" "${API_BASE}/apps/import" 2>/dev/null) \
    || die "Failed to import map from '${file}'"
  local app_name
  app_name=$(echo "$resp" | jq -r '.application_name // .name // "unknown"')
  ok "Imported application: ${app_name}"
}

# ── Component Catalog ────────────────────────────────────────────────────────

# Seed the built-in component types into the catalog for the current org.
cmd_init_catalog() {
  ensure_login
  info "Seeding built-in component types..."
  local resp
  resp=$(api_post "${API_BASE}/catalog/component-types/seed" "{}") \
    || die "Failed to seed catalog"
  local seeded
  seeded=$(echo "$resp" | jq -r '.seeded // 0')
  ok "Catalog initialized (${seeded} types seeded)"
}

# Import custom component types from a JSON file.
#
# Expected format:
# {
#   "entries": [
#     {
#       "type_key": "oracle-rac",
#       "label": "Oracle RAC Cluster",
#       "description": "Oracle Real Application Clusters",
#       "icon": "database",
#       "color": "#F80000",
#       "category": "database",
#       "default_check_cmd": "srvctl status database -d ${DB_NAME}",
#       "default_start_cmd": "srvctl start database -d ${DB_NAME}",
#       "default_stop_cmd": "srvctl stop database -d ${DB_NAME}"
#     }
#   ]
# }
cmd_import_catalog() {
  local file=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --file) file="$2"; shift 2 ;;
      *)      file="$1"; shift ;;
    esac
  done
  [ -n "$file" ] || die "Missing file. Usage: import-catalog --file catalog.json"
  [ -f "$file" ] || die "File not found: $file"
  ensure_login

  info "Importing catalog from ${file}..."
  local resp
  resp=$(curl -sf -X POST -H "Authorization: Bearer ${TOKEN}" \
    -H "Content-Type: application/json" \
    -d @"$file" "${API_BASE}/catalog/component-types/import" 2>/dev/null) \
    || die "Failed to import catalog from '${file}'"

  local created skipped total
  created=$(echo "$resp" | jq -r '.created // 0')
  skipped=$(echo "$resp" | jq -r '.skipped // 0')
  total=$(echo "$resp" | jq -r '.total // 0')
  ok "Catalog imported: ${created} created, ${skipped} skipped (${total} total)"
}

# List component types in the catalog.
cmd_list_catalog() {
  ensure_login
  local resp
  resp=$(api_get "${API_BASE}/catalog/component-types") || die "Failed to list catalog"
  echo "$resp" | jq -r '.entries[] | "\(.type_key)\t\(.label)\t\(.icon)\t\(.color)\t\(.category // "-")\t\(.is_builtin)"' 2>/dev/null \
    | column -t -s $'\t' -N "TYPE_KEY,LABEL,ICON,COLOR,CATEGORY,BUILTIN"
}

# ── Provision (bulk) ─────────────────────────────────────────────────────────
# Reads a JSON file with users, teams, and permissions and applies them all.
#
# Expected format:
# {
#   "users": [
#     { "email": "alice@corp.com", "name": "Alice", "role": "operator", "password": "changeme" }
#   ],
#   "teams": [
#     { "name": "Prod Team", "description": "...", "members": ["alice@corp.com"] }
#   ],
#   "permissions": [
#     { "app": "My App", "team": "Prod Team", "level": "operate" },
#     { "app": "My App", "user": "alice@corp.com", "level": "manage" }
#   ]
# }

cmd_provision() {
  local file=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --file) file="$2"; shift 2 ;;
      *)      file="$1"; shift ;;
    esac
  done
  [ -n "$file" ] || die "Missing file. Usage: provision --file setup.json"
  [ -f "$file" ] || die "File not found: $file"
  ensure_login

  info "Provisioning from ${file}..."

  # ── Users ──
  local user_count
  user_count=$(jq '.users | length' "$file" 2>/dev/null || echo 0)
  if [ "$user_count" -gt 0 ]; then
    info "Creating ${user_count} users..."
    for i in $(seq 0 $((user_count - 1))); do
      local u_email u_name u_role u_password
      u_email=$(jq -r ".users[$i].email" "$file")
      u_name=$(jq -r ".users[$i].name" "$file")
      u_role=$(jq -r ".users[$i].role // \"operator\"" "$file")
      u_password=$(jq -r ".users[$i].password // empty" "$file")

      local args=(--email "$u_email" --name "$u_name" --role "$u_role")
      [ -n "$u_password" ] && args+=(--password "$u_password")
      cmd_create_user "${args[@]}" > /dev/null
    done
  fi

  # ── Teams ──
  local team_count
  team_count=$(jq '.teams | length' "$file" 2>/dev/null || echo 0)
  if [ "$team_count" -gt 0 ]; then
    info "Creating ${team_count} teams..."
    for i in $(seq 0 $((team_count - 1))); do
      local t_name t_desc
      t_name=$(jq -r ".teams[$i].name" "$file")
      t_desc=$(jq -r ".teams[$i].description // empty" "$file")

      cmd_create_team --name "$t_name" --description "${t_desc:-}" > /dev/null

      # Add members
      local member_count
      member_count=$(jq ".teams[$i].members | length" "$file" 2>/dev/null || echo 0)
      for j in $(seq 0 $((member_count - 1))); do
        local m_email
        m_email=$(jq -r ".teams[$i].members[$j]" "$file")
        cmd_add_member --team "$t_name" --email "$m_email" 2>/dev/null || true
      done
    done
  fi

  # ── Permissions ──
  local perm_count
  perm_count=$(jq '.permissions | length' "$file" 2>/dev/null || echo 0)
  if [ "$perm_count" -gt 0 ]; then
    info "Granting ${perm_count} permissions..."
    for i in $(seq 0 $((perm_count - 1))); do
      local p_app p_team p_user p_level
      p_app=$(jq -r ".permissions[$i].app" "$file")
      p_team=$(jq -r ".permissions[$i].team // empty" "$file")
      p_user=$(jq -r ".permissions[$i].user // empty" "$file")
      p_level=$(jq -r ".permissions[$i].level // \"view\"" "$file")

      if [ -n "$p_team" ]; then
        cmd_grant_access --app "$p_app" --team "$p_team" --level "$p_level"
      elif [ -n "$p_user" ]; then
        cmd_grant_access --app "$p_app" --user "$p_user" --level "$p_level"
      else
        warn "Permission entry $i has neither 'team' nor 'user' — skipping"
      fi
    done
  fi

  echo ""
  ok "Provisioning complete!"
  info "Summary: ${user_count} users, ${team_count} teams, ${perm_count} permissions"
}

# ── Usage ────────────────────────────────────────────────────────────────────

usage() {
  cat <<'USAGE'
AppControl CLI — Provisioning & Management

Usage: appcontrol.sh <command> [options]

Commands:
  create-user     --email EMAIL --name NAME [--role operator] [--password PWD]
  create-team     --name NAME [--description DESC]
  add-member      --team TEAM_NAME --email USER_EMAIL [--role member|lead]
  grant-access    --app APP_NAME (--team TEAM | --user EMAIL) --level LEVEL
  list-users      List all users
  list-teams      List all teams
  list-apps       List all applications
  import-map      --file MAP.json
  provision       --file setup.json  (bulk: users + teams + permissions)
  init-catalog    Seed built-in component types into the catalog
  import-catalog  --file catalog.json  Import custom component types
  list-catalog    List all component types in the catalog

Permission levels: view, operate, edit, manage, owner

Environment variables:
  APPCONTROL_URL       Backend URL (default: http://localhost:3001)
  APPCONTROL_EMAIL     Admin email (default: admin@localhost)
  APPCONTROL_PASSWORD  Admin password (default: admin)

Examples:
  # Create a user
  appcontrol.sh create-user --email alice@corp.com --name "Alice Martin" --password changeme

  # Create a team and add members
  appcontrol.sh create-team --name "Equipe Prod"
  appcontrol.sh add-member --team "Equipe Prod" --email alice@corp.com

  # Grant team access to an application
  appcontrol.sh grant-access --app "Core Banking" --team "Equipe Prod" --level operate

  # Bulk provision from JSON
  appcontrol.sh provision --file setup.json

  # Initialize the component catalog with built-in types
  appcontrol.sh init-catalog

  # Import custom component types from a JSON file
  appcontrol.sh import-catalog --file my-types.json
USAGE
}

# ── Main ─────────────────────────────────────────────────────────────────────

[ $# -gt 0 ] || { usage; exit 0; }

command="$1"; shift
case "$command" in
  create-user)    cmd_create_user "$@" ;;
  create-team)    cmd_create_team "$@" ;;
  add-member)     cmd_add_member "$@" ;;
  grant-access)   cmd_grant_access "$@" ;;
  list-users)     cmd_list_users "$@" ;;
  list-teams)     cmd_list_teams "$@" ;;
  list-apps)      cmd_list_apps "$@" ;;
  import-map)     cmd_import_map "$@" ;;
  provision)      cmd_provision "$@" ;;
  init-catalog)   cmd_init_catalog "$@" ;;
  import-catalog) cmd_import_catalog "$@" ;;
  list-catalog)   cmd_list_catalog "$@" ;;
  help|--help|-h) usage ;;
  *) die "Unknown command: $command. Run 'appcontrol.sh help' for usage." ;;
esac
