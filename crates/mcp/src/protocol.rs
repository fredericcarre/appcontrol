//! MCP JSON-RPC 2.0 protocol helpers.

use serde_json::{json, Value};

/// Server capabilities response to `initialize`.
pub fn initialize_response(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "appcontrol",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

/// List all available tools.
pub fn tools_list_response(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": tool_definitions()
        }
    })
}

/// Successful tool result.
pub fn tool_result_response(id: Value, content: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [
                {
                    "type": "text",
                    "text": content
                }
            ]
        }
    })
}

/// Tool execution error.
pub fn tool_error_response(id: Value, error_msg: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [
                {
                    "type": "text",
                    "text": format!("Error: {}", error_msg)
                }
            ],
            "isError": true
        }
    })
}

/// JSON-RPC error response.
pub fn error_response(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

/// Pong response to ping.
pub fn pong_response(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {}
    })
}

/// All MCP tool definitions.
fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "list_apps",
            "description": "List all applications managed by AppControl. Returns name, status, site, and component count for each application.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        }),
        json!({
            "name": "get_app_status",
            "description": "Get detailed status of a specific application, including all component states, health summary, and recent events.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app_name": {
                        "type": "string",
                        "description": "Application name or ID"
                    }
                },
                "required": ["app_name"]
            }
        }),
        json!({
            "name": "start_app",
            "description": "Start an application. Respects DAG dependencies: starts components in the correct order. Can optionally do a dry-run to preview the execution plan.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app_name": {
                        "type": "string",
                        "description": "Application name or ID"
                    },
                    "dry_run": {
                        "type": "boolean",
                        "description": "If true, shows the plan without executing"
                    }
                },
                "required": ["app_name"]
            }
        }),
        json!({
            "name": "stop_app",
            "description": "Stop an application. Stops components in reverse DAG order (children first, then parents).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app_name": {
                        "type": "string",
                        "description": "Application name or ID"
                    }
                },
                "required": ["app_name"]
            }
        }),
        json!({
            "name": "diagnose_app",
            "description": "Run a 3-level diagnostic on an application: Level 1 (health), Level 2 (data integrity), Level 3 (infrastructure). Returns per-component recommendations (Healthy, Restart, AppRebuild, InfraRebuild).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app_name": {
                        "type": "string",
                        "description": "Application name or ID"
                    }
                },
                "required": ["app_name"]
            }
        }),
        json!({
            "name": "get_incidents",
            "description": "Get recent incidents for an application. Shows FAILED state transitions, durations, and affected components.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app_name": {
                        "type": "string",
                        "description": "Application name or ID"
                    },
                    "days": {
                        "type": "integer",
                        "description": "Number of days to look back (default: 7)"
                    }
                },
                "required": ["app_name"]
            }
        }),
        json!({
            "name": "get_topology",
            "description": "Get the dependency graph (DAG) of an application as a list of components and edges. Useful for understanding application architecture.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app_name": {
                        "type": "string",
                        "description": "Application name or ID"
                    }
                },
                "required": ["app_name"]
            }
        }),
        json!({
            "name": "estimate_time",
            "description": "Estimate how long a start, stop, or restart operation will take based on historical execution data. Returns typical (P50) and worst-case (P95) estimates.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app_name": {
                        "type": "string",
                        "description": "Application name or ID"
                    },
                    "operation": {
                        "type": "string",
                        "description": "Operation type: start, stop, or restart",
                        "enum": ["start", "stop", "restart"]
                    }
                },
                "required": ["app_name"]
            }
        }),
        json!({
            "name": "get_activity",
            "description": "Get recent activity feed for an application: state changes, commands executed, user actions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app_name": {
                        "type": "string",
                        "description": "Application name or ID"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Number of events to return (default: 20)"
                    }
                },
                "required": ["app_name"]
            }
        }),
        json!({
            "name": "list_agents",
            "description": "List all registered agents with their connection status, hostname, and last heartbeat time.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        }),
        // Log access tools
        json!({
            "name": "list_log_sources",
            "description": "List all declared log sources for a component. Sources can be files, Windows Event Logs, or diagnostic commands. Use this to discover available logs before fetching them.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app_name": {
                        "type": "string",
                        "description": "Application name or ID"
                    },
                    "component_name": {
                        "type": "string",
                        "description": "Component name within the application"
                    }
                },
                "required": ["app_name", "component_name"]
            }
        }),
        json!({
            "name": "get_component_logs",
            "description": "Get logs from a component. By default returns process stdout/stderr (console output). Can also fetch from declared log sources (files, event logs). Use list_log_sources first to discover available sources.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app_name": {
                        "type": "string",
                        "description": "Application name or ID"
                    },
                    "component_name": {
                        "type": "string",
                        "description": "Component name within the application"
                    },
                    "source": {
                        "type": "string",
                        "description": "Log source: 'process' (default), or a source ID from list_log_sources"
                    },
                    "lines": {
                        "type": "integer",
                        "description": "Number of lines to return (default: 100, max: 1000)"
                    },
                    "filter": {
                        "type": "string",
                        "description": "Filter logs by text or level (ERROR, WARN, INFO)"
                    },
                    "since": {
                        "type": "string",
                        "description": "Time range: '1h', '6h', '24h', '7d'"
                    }
                },
                "required": ["app_name", "component_name"]
            }
        }),
        json!({
            "name": "run_diagnostic_command",
            "description": "Execute a diagnostic command declared for a component. Use list_log_sources first to discover available commands (source_type='command'). Commands are read-only diagnostic tools like 'rabbitmqctl status', 'netstat', etc.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app_name": {
                        "type": "string",
                        "description": "Application name or ID"
                    },
                    "component_name": {
                        "type": "string",
                        "description": "Component name within the application"
                    },
                    "command_name": {
                        "type": "string",
                        "description": "Name of the diagnostic command (from list_log_sources)"
                    }
                },
                "required": ["app_name", "component_name", "command_name"]
            }
        }),
        json!({
            "name": "search_logs",
            "description": "Search for errors or patterns across all components of an application. Returns matching log entries with component names and timestamps. Useful for debugging cross-component issues.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app_name": {
                        "type": "string",
                        "description": "Application name or ID"
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Search pattern (supports regex)"
                    },
                    "level": {
                        "type": "string",
                        "description": "Filter by log level: ERROR, WARN, INFO",
                        "enum": ["ERROR", "WARN", "INFO"]
                    },
                    "since": {
                        "type": "string",
                        "description": "Time range: '1h', '6h', '24h', '7d'",
                        "default": "1h"
                    }
                },
                "required": ["app_name", "pattern"]
            }
        }),
    ]
}
