// MCP (Model Context Protocol) server for AI integration.
//
// The MCP server is implemented as a standalone binary in crates/mcp/.
// It connects to the backend REST API and exposes 10 tools for AI-driven
// operations management:
//
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
