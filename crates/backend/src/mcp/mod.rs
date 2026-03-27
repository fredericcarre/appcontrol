// MCP (Model Context Protocol) server for AI integration.
//
// The MCP server is implemented as a standalone binary in crates/mcp/.
// It connects to the backend REST API and exposes 14 tools for AI-driven
// operations management:
//
// === Application Operations ===
// 1. list_apps        — List all managed applications
// 2. get_app_status   — Get detailed status with component states
// 3. start_app        — Start application (DAG-ordered, with dry-run)
// 4. stop_app         — Stop application (reverse DAG order)
// 5. diagnose_app     — 3-level diagnostic assessment
// 6. get_incidents    — Recent incidents and FAILED transitions
// 7. get_topology     — Dependency graph as components + edges
// 8. estimate_time    — Operation time estimation (P50/P95)
// 9. get_activity     — Real-time activity feed
// 10. list_agents     — Registered agent status
//
// === Log Access & Diagnostics ===
// 11. list_log_sources       — List declared log sources for a component
// 12. get_component_logs     — Get logs (process output, files, event logs)
// 13. run_diagnostic_command — Execute diagnostic command on remote agent
// 14. search_logs            — Search for errors across all components
//
// These log tools enable Claude to:
// - View process stdout/stderr captured by AppControl
// - Read log files on remote VMs
// - Access Windows Event Log entries
// - Run diagnostic commands (rabbitmqctl status, netstat, etc.)
// - Search for errors across all components of an application
//
// Usage:
//   appcontrol-mcp --url https://appcontrol.corp.com --api-key ac_xxxxx
//
// Or as a Claude Desktop MCP server:
//   {
//     "mcpServers": {
//       "appcontrol": {
//         "command": "appcontrol-mcp",
//         "args": ["--url", "https://appcontrol.corp.com", "--api-key", "ac_xxxxx"]
//       }
//     }
//   }
//
// Remote Access Flow:
//   Claude → MCP Server → Backend API → Gateway → Agent (remote VM)
//
// Each tool call is authenticated via API key and respects permission levels:
// - View: list_apps, get_app_status, get_topology, list_agents
// - Operate: start_app, stop_app, get_component_logs, run_diagnostic_command
// - Edit: access to sensitive log sources
