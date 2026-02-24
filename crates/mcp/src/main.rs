//! AppControl MCP Server — Model Context Protocol for AI-driven operations.
//!
//! This is a standalone binary that implements the MCP protocol over stdin/stdout
//! (JSON-RPC 2.0). It connects to the AppControl backend REST API and exposes
//! operations as MCP "tools" that any MCP-compatible AI client (Claude Desktop,
//! Cursor, etc.) can invoke via natural language.
//!
//! Configuration:
//!   appcontrol-mcp --url https://appcontrol.corp.com --api-key ac_xxxxx
//!
//! Or via environment variables:
//!   APPCONTROL_URL=https://appcontrol.corp.com
//!   APPCONTROL_API_KEY=ac_xxxxx

mod protocol;
mod tools;

use clap::Parser;
use std::io::{self, BufRead, Write};

#[derive(Parser, Debug)]
#[command(
    name = "appcontrol-mcp",
    about = "AppControl MCP Server for AI integration"
)]
struct Args {
    /// AppControl backend URL
    #[arg(long, env = "APPCONTROL_URL", default_value = "http://localhost:3001")]
    url: String,

    /// API key for authentication
    #[arg(long, env = "APPCONTROL_API_KEY")]
    api_key: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // MCP servers MUST NOT log to stdout (that's the JSON-RPC channel).
    // Log to stderr instead.
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter("appcontrol_mcp=debug")
        .init();

    tracing::info!("AppControl MCP server starting (backend={})", args.url);

    let client = tools::McpClient::new(&args.url, &args.api_key)?;

    // Read JSON-RPC messages from stdin, write responses to stdout
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        let request: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let error_response = protocol::error_response(
                    serde_json::Value::Null,
                    -32700,
                    &format!("Parse error: {}", e),
                );
                writeln!(stdout, "{}", serde_json::to_string(&error_response)?)?;
                stdout.flush()?;
                continue;
            }
        };

        let response = handle_request(&client, &request).await;

        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
        stdout.flush()?;
    }

    Ok(())
}

async fn handle_request(
    client: &tools::McpClient,
    request: &serde_json::Value,
) -> serde_json::Value {
    let id = request
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

    match method {
        "initialize" => protocol::initialize_response(id),
        "initialized" => serde_json::Value::Null, // notification, no response
        "tools/list" => protocol::tools_list_response(id),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or_default();
            let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::json!({}));

            match client.call_tool(tool_name, &arguments).await {
                Ok(result) => protocol::tool_result_response(id, &result),
                Err(e) => protocol::tool_error_response(id, &e.to_string()),
            }
        }
        "ping" => protocol::pong_response(id),
        _ => protocol::error_response(id, -32601, &format!("Method not found: {}", method)),
    }
}
