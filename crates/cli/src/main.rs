use clap::{Parser, Subcommand};
use serde_json::Value;

#[derive(Parser)]
#[command(name = "appctl", about = "AppControl CLI — scheduler integration")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Backend URL
    #[arg(long, env = "APPCONTROL_URL", default_value = "http://localhost:8080")]
    url: String,

    /// API key
    #[arg(long, env = "APPCONTROL_API_KEY")]
    api_key: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start an application
    Start {
        /// Application name or ID
        app: String,
        /// Wait for all components to reach RUNNING
        #[arg(long)]
        wait: bool,
        /// Timeout in seconds (with --wait)
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Dry run (show plan without executing)
        #[arg(long)]
        dry_run: bool,
    },
    /// Stop an application
    Stop {
        app: String,
        #[arg(long)]
        wait: bool,
        #[arg(long, default_value = "300")]
        timeout: u64,
    },
    /// Show application status
    Status {
        app: String,
        #[arg(long, default_value = "table")]
        format: String,
    },
    /// DR site switchover
    Switchover {
        app: String,
        #[arg(long)]
        target_site: String,
        #[arg(long, default_value = "FULL")]
        mode: String,
        #[arg(long)]
        wait: bool,
    },
    /// Run 3-level diagnostic
    Diagnose {
        app: String,
        #[arg(long, default_value = "table")]
        format: String,
    },
    /// Rebuild application components
    Rebuild {
        app: String,
        #[arg(long)]
        components: Option<Vec<String>>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        wait: bool,
    },
}

/// Exit codes (critical for scheduler integration)
const EXIT_SUCCESS: i32 = 0;
const EXIT_FAILURE: i32 = 1;
const EXIT_TIMEOUT: i32 = 2;
const EXIT_AUTH_ERROR: i32 = 3;
const EXIT_NOT_FOUND: i32 = 4;
const EXIT_PERMISSION_DENIED: i32 = 5;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let client = build_client(&cli);

    let exit_code = match cli.command {
        Commands::Start {
            app,
            wait,
            timeout,
            dry_run,
        } => cmd_start(&client, &cli.url, &app, wait, timeout, dry_run).await,
        Commands::Stop { app, wait, timeout } => {
            cmd_stop(&client, &cli.url, &app, wait, timeout).await
        }
        Commands::Status { app, format } => cmd_status(&client, &cli.url, &app, &format).await,
        Commands::Switchover {
            app,
            target_site,
            mode,
            wait,
        } => cmd_switchover(&client, &cli.url, &app, &target_site, &mode, wait).await,
        Commands::Diagnose { app, format } => cmd_diagnose(&client, &cli.url, &app, &format).await,
        Commands::Rebuild {
            app,
            components,
            dry_run,
            wait,
        } => cmd_rebuild(&client, &cli.url, &app, components, dry_run, wait).await,
    };

    std::process::exit(exit_code);
}

fn build_client(cli: &Cli) -> reqwest::Client {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(ref key) = cli.api_key {
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!("ApiKey {}", key)).unwrap(),
        );
    }

    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap()
}

async fn cmd_start(
    client: &reqwest::Client,
    base: &str,
    app: &str,
    wait: bool,
    timeout: u64,
    dry_run: bool,
) -> i32 {
    let url = format!("{}/api/v1/orchestration/apps/{}/start", base, app);
    let body = serde_json::json!({ "dry_run": dry_run });

    match client.post(&url).json(&body).send().await {
        Ok(resp) => {
            let status = resp.status();
            match status.as_u16() {
                200 | 201 => {
                    let body: Value = resp.json().await.unwrap_or_default();
                    println!("{}", serde_json::to_string_pretty(&body).unwrap());

                    if wait && !dry_run {
                        return wait_running(client, base, app, timeout).await;
                    }
                    EXIT_SUCCESS
                }
                401 => {
                    eprintln!("Authentication error");
                    EXIT_AUTH_ERROR
                }
                403 => {
                    eprintln!("Permission denied");
                    EXIT_PERMISSION_DENIED
                }
                404 => {
                    eprintln!("Application not found: {}", app);
                    EXIT_NOT_FOUND
                }
                _ => {
                    eprintln!("Error: HTTP {}", status);
                    EXIT_FAILURE
                }
            }
        }
        Err(e) => {
            eprintln!("Connection error: {}", e);
            EXIT_FAILURE
        }
    }
}

async fn cmd_stop(
    client: &reqwest::Client,
    base: &str,
    app: &str,
    wait: bool,
    timeout: u64,
) -> i32 {
    let url = format!("{}/api/v1/orchestration/apps/{}/stop", base, app);

    match client.post(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            match status.as_u16() {
                200 => {
                    let body: Value = resp.json().await.unwrap_or_default();
                    println!("{}", serde_json::to_string_pretty(&body).unwrap());

                    if wait {
                        return wait_stopped(client, base, app, timeout).await;
                    }
                    EXIT_SUCCESS
                }
                401 => EXIT_AUTH_ERROR,
                403 => EXIT_PERMISSION_DENIED,
                404 => EXIT_NOT_FOUND,
                _ => EXIT_FAILURE,
            }
        }
        Err(_) => EXIT_FAILURE,
    }
}

async fn cmd_status(client: &reqwest::Client, base: &str, app: &str, format: &str) -> i32 {
    let url = format!("{}/api/v1/orchestration/apps/{}/status", base, app);

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: Value = resp.json().await.unwrap_or_default();

            match format {
                "json" => println!("{}", serde_json::to_string_pretty(&body).unwrap()),
                "short" => {
                    if let Some(all_running) = body.get("all_running") {
                        println!(
                            "{}",
                            if all_running.as_bool().unwrap_or(false) {
                                "RUNNING"
                            } else {
                                "NOT_RUNNING"
                            }
                        );
                    }
                }
                _ => {
                    // Table format
                    let mut table = comfy_table::Table::new();
                    table.set_header(vec!["Component", "State"]);
                    if let Some(components) = body.get("components").and_then(|c| c.as_array()) {
                        for comp in components {
                            table.add_row(vec![
                                comp.get("name").and_then(|n| n.as_str()).unwrap_or("?"),
                                comp.get("state").and_then(|s| s.as_str()).unwrap_or("?"),
                            ]);
                        }
                    }
                    println!("{}", table);
                }
            }
            EXIT_SUCCESS
        }
        Ok(resp) => {
            let status = resp.status().as_u16();
            match status {
                401 => EXIT_AUTH_ERROR,
                403 => EXIT_PERMISSION_DENIED,
                404 => EXIT_NOT_FOUND,
                _ => EXIT_FAILURE,
            }
        }
        Err(_) => EXIT_FAILURE,
    }
}

async fn cmd_switchover(
    client: &reqwest::Client,
    base: &str,
    app: &str,
    target_site: &str,
    mode: &str,
    _wait: bool,
) -> i32 {
    let url = format!("{}/api/v1/apps/{}/switchover", base, app);
    let body = serde_json::json!({ "target_site_id": target_site, "mode": mode });

    match client.post(&url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: Value = resp.json().await.unwrap_or_default();
            println!("{}", serde_json::to_string_pretty(&body).unwrap());
            EXIT_SUCCESS
        }
        _ => EXIT_FAILURE,
    }
}

async fn cmd_diagnose(client: &reqwest::Client, base: &str, app: &str, format: &str) -> i32 {
    let url = format!("{}/api/v1/apps/{}/diagnose", base, app);

    match client.post(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: Value = resp.json().await.unwrap_or_default();

            match format {
                "json" => println!("{}", serde_json::to_string_pretty(&body).unwrap()),
                _ => {
                    let mut table = comfy_table::Table::new();
                    table.set_header(vec![
                        "Component",
                        "Health",
                        "Integrity",
                        "Infrastructure",
                        "Recommendation",
                    ]);
                    if let Some(diagnosis) = body.get("diagnosis").and_then(|d| d.as_array()) {
                        for d in diagnosis {
                            table.add_row(vec![
                                d.get("component_name")
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("?"),
                                d.get("health").and_then(|h| h.as_str()).unwrap_or("?"),
                                d.get("integrity").and_then(|i| i.as_str()).unwrap_or("?"),
                                d.get("infrastructure")
                                    .and_then(|i| i.as_str())
                                    .unwrap_or("?"),
                                d.get("recommendation")
                                    .and_then(|r| r.as_str())
                                    .unwrap_or("?"),
                            ]);
                        }
                    }
                    println!("{}", table);
                }
            }
            EXIT_SUCCESS
        }
        _ => EXIT_FAILURE,
    }
}

async fn cmd_rebuild(
    client: &reqwest::Client,
    base: &str,
    app: &str,
    components: Option<Vec<String>>,
    dry_run: bool,
    _wait: bool,
) -> i32 {
    let url = format!("{}/api/v1/apps/{}/rebuild", base, app);
    let body = serde_json::json!({ "component_ids": components, "dry_run": dry_run });

    match client.post(&url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: Value = resp.json().await.unwrap_or_default();
            println!("{}", serde_json::to_string_pretty(&body).unwrap());
            EXIT_SUCCESS
        }
        _ => EXIT_FAILURE,
    }
}

async fn wait_running(client: &reqwest::Client, base: &str, app: &str, timeout: u64) -> i32 {
    let url = format!(
        "{}/api/v1/orchestration/apps/{}/wait-running?timeout={}",
        base, app, timeout
    );

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: Value = resp.json().await.unwrap_or_default();
            match body.get("status").and_then(|s| s.as_str()) {
                Some("running") => EXIT_SUCCESS,
                Some("timeout") => {
                    eprintln!("Timeout waiting for RUNNING");
                    EXIT_TIMEOUT
                }
                Some("failed") => {
                    eprintln!("Application failed");
                    EXIT_FAILURE
                }
                _ => EXIT_FAILURE,
            }
        }
        _ => EXIT_FAILURE,
    }
}

async fn wait_stopped(client: &reqwest::Client, base: &str, app: &str, timeout: u64) -> i32 {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
    let url = format!("{}/api/v1/orchestration/apps/{}/status", base, app);

    loop {
        if std::time::Instant::now() > deadline {
            return EXIT_TIMEOUT;
        }

        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body: Value = resp.json().await.unwrap_or_default();
                if let Some(components) = body.get("components").and_then(|c| c.as_array()) {
                    let all_stopped = components
                        .iter()
                        .all(|c| c.get("state").and_then(|s| s.as_str()) == Some("STOPPED"));
                    if all_stopped {
                        return EXIT_SUCCESS;
                    }
                }
            }
            _ => {}
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
