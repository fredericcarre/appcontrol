//! `appcontrol-ai` — the AI layer CLI.
//!
//! Runnable standalone, no backend required. The flagship command turns agent
//! discovery into a readable architecture map:
//!
//! ```text
//! appcontrol-agent discover --json | appcontrol-ai architect
//! appcontrol-ai demo            # uses a bundled sample, no infra at all
//! appcontrol-ai classify "spring.datasource.password=secret"
//! ```

use std::io::Read;

use appcontrol_ai::{
    architect::{self, Fragment},
    config::router_from_env,
    render,
    sensitivity::SensitivityClassifier,
    types::ArchitectureView,
};
use appcontrol_common::protocol::AgentMessage;
use clap::{Parser, Subcommand};

/// A bundled, realistic discovery report (an "order platform" + system noise)
/// so `demo` works with zero infrastructure.
const SAMPLE: &str = include_str!("../../sample/discovery-sample.json");

#[derive(Parser)]
#[command(
    name = "appcontrol-ai",
    about = "AppControl AI layer — sovereign inference + architect map"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a readable architecture map from agent discovery (stdin or --input).
    Architect {
        /// Read discovery JSON from this file instead of stdin.
        #[arg(long)]
        input: Option<String>,
        /// Emit the ArchitectureView as JSON instead of the text diagram.
        #[arg(long)]
        json: bool,
        /// Skip the optional LLM naming pass (pure deterministic output).
        #[arg(long)]
        no_ai: bool,
    },
    /// Run the architect on a bundled sample — no infrastructure needed.
    Demo {
        #[arg(long)]
        json: bool,
    },
    /// Show how the sovereign router would classify a piece of text.
    Classify {
        /// The text to classify.
        text: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "appcontrol_ai=info".into()),
        )
        .init();

    let args = Args::parse();
    match args.command {
        Command::Architect { input, json, no_ai } => {
            let raw = match input {
                Some(path) => std::fs::read_to_string(path)?,
                None => {
                    let mut buf = String::new();
                    std::io::stdin().read_to_string(&mut buf)?;
                    buf
                }
            };
            run_architect(&raw, json, no_ai).await
        }
        Command::Demo { json } => run_architect(SAMPLE, json, false).await,
        Command::Classify { text } => {
            let c = SensitivityClassifier;
            let s = c.classify(&text);
            println!("sensitivity = {s:?}");
            println!(
                "routing     = {}",
                match s {
                    appcontrol_ai::Sensitivity::Public | appcontrol_ai::Sensitivity::Internal =>
                        "may go to a frontier model (after redaction)",
                    _ => "pinned to a local/sovereign model — never leaves the machine",
                }
            );
            Ok(())
        }
    }
}

async fn run_architect(raw: &str, json: bool, no_ai: bool) -> anyhow::Result<()> {
    let fragments = parse_fragments(raw)?;
    if fragments.is_empty() {
        anyhow::bail!("no DiscoveryReport found in input");
    }
    let mut view = architect::build(&fragments);

    if !no_ai {
        // Routed through the sovereign router; with no LLM configured this is a
        // deterministic no-op (mock), so the demo still works offline.
        let router = router_from_env();
        let _ = architect::name_groups(&mut view, &router).await;
    }

    emit(&view, json)
}

/// Parse one or more discovery reports from JSON. Accepts a single object or an
/// array (multi-agent aggregation).
fn parse_fragments(raw: &str) -> anyhow::Result<Vec<Fragment>> {
    let value: serde_json::Value = serde_json::from_str(raw)?;
    let messages: Vec<AgentMessage> = match value {
        serde_json::Value::Array(_) => serde_json::from_value(value)?,
        _ => vec![serde_json::from_value(value)?],
    };
    Ok(messages
        .into_iter()
        .filter_map(Fragment::from_message)
        .collect())
}

fn emit(view: &ArchitectureView, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
    } else {
        print!("{}", render::to_text(view));
    }
    Ok(())
}
