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
    /// PKI and enrollment management
    Pki {
        #[command(subcommand)]
        action: PkiCommands,
    },
}

#[derive(Subcommand)]
enum PkiCommands {
    /// Initialize the CA for this organization (one-time setup)
    Init {
        /// Organization name for the CA
        #[arg(long)]
        org_name: String,
        /// CA validity in days (default 3650 = 10 years)
        #[arg(long, default_value = "3650")]
        validity_days: u32,
        /// Output directory for CA files (optional — also stored in backend DB)
        #[arg(long)]
        out: Option<String>,
    },
    /// Create an enrollment token for agents/gateways
    CreateToken {
        /// Human-readable name for this token
        #[arg(long)]
        name: String,
        /// Max number of uses (default: unlimited)
        #[arg(long)]
        max_uses: Option<i32>,
        /// Validity in hours (default 24)
        #[arg(long, default_value = "24")]
        valid_hours: i64,
        /// Token scope: "agent" or "gateway" (default "agent")
        #[arg(long, default_value = "agent")]
        scope: String,
    },
    /// List enrollment tokens
    ListTokens,
    /// Revoke an enrollment token
    RevokeToken {
        /// Token ID to revoke
        id: String,
    },
    /// Issue an agent certificate locally (requires CA key)
    IssueAgent {
        /// Agent hostname
        #[arg(long)]
        hostname: String,
        /// Path to CA cert PEM
        #[arg(long)]
        ca_cert: String,
        /// Path to CA key PEM
        #[arg(long)]
        ca_key: String,
        /// Output directory for agent cert files
        #[arg(long, default_value = ".")]
        out: String,
        /// Validity in days (default 365)
        #[arg(long, default_value = "365")]
        validity_days: u32,
    },
    /// Issue a gateway certificate locally (requires CA key)
    IssueGateway {
        /// Gateway hostname / CN
        #[arg(long)]
        cn: String,
        /// Additional DNS SANs
        #[arg(long)]
        san_dns: Vec<String>,
        /// Additional IP SANs
        #[arg(long)]
        san_ip: Vec<String>,
        /// Path to CA cert PEM
        #[arg(long)]
        ca_cert: String,
        /// Path to CA key PEM
        #[arg(long)]
        ca_key: String,
        /// Output directory
        #[arg(long, default_value = ".")]
        out: String,
        /// Validity in days (default 365)
        #[arg(long, default_value = "365")]
        validity_days: u32,
    },
    /// Show enrollment events (audit trail)
    Events,
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
        Commands::Pki { action } => cmd_pki(&client, &cli.url, action).await,
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

async fn cmd_pki(client: &reqwest::Client, base: &str, action: PkiCommands) -> i32 {
    match action {
        PkiCommands::Init {
            org_name,
            validity_days,
            out,
        } => {
            // Call backend to init PKI (stores CA in DB)
            let url = format!("{}/api/v1/pki/init", base);
            let body = serde_json::json!({
                "org_name": org_name,
                "validity_days": validity_days,
            });

            match client.post(&url).json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let body: Value = resp.json().await.unwrap_or_default();
                    let fingerprint = body
                        .get("ca_fingerprint")
                        .and_then(|f| f.as_str())
                        .unwrap_or("?");

                    println!("  AppControl PKI Bootstrap");
                    println!("  ========================");
                    println!();
                    println!("  CA fingerprint: {}", fingerprint);
                    println!("  Validity: {} days", validity_days);
                    println!("  Stored in backend database");

                    // Optionally write CA cert to disk
                    if let Some(ref out_dir) = out {
                        if let Some(ca_cert) = body.get("ca_cert_pem").and_then(|c| c.as_str()) {
                            let dir = std::path::Path::new(out_dir);
                            std::fs::create_dir_all(dir).ok();
                            let ca_path = dir.join("ca.crt");
                            if std::fs::write(&ca_path, ca_cert).is_ok() {
                                println!("  CA cert written: {}", ca_path.display());
                            }
                        }
                    }

                    println!();
                    println!("  Next steps:");
                    println!("  1. Create enrollment tokens: appctl pki create-token --name \"my-deploy\"");
                    println!("  2. Enroll agents: appcontrol-agent --enroll <gateway-url> --token <token>");

                    EXIT_SUCCESS
                }
                Ok(resp) => {
                    let status = resp.status();
                    let body: Value = resp.json().await.unwrap_or_default();
                    let msg = body
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown error");
                    eprintln!("Error (HTTP {}): {}", status, msg);
                    EXIT_FAILURE
                }
                Err(e) => {
                    eprintln!("Connection error: {}", e);
                    EXIT_FAILURE
                }
            }
        }
        PkiCommands::CreateToken {
            name,
            max_uses,
            valid_hours,
            scope,
        } => {
            let url = format!("{}/api/v1/enrollment/tokens", base);
            let body = serde_json::json!({
                "name": name,
                "max_uses": max_uses,
                "valid_hours": valid_hours,
                "scope": scope,
            });

            match client.post(&url).json(&body).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let body: Value = resp.json().await.unwrap_or_default();
                    let token = body.get("token").and_then(|t| t.as_str()).unwrap_or("?");
                    let expires = body
                        .get("expires_at")
                        .and_then(|e| e.as_str())
                        .unwrap_or("?");

                    println!("  Enrollment Token Created");
                    println!("  ========================");
                    println!();
                    println!("  Token: {}", token);
                    println!("  Name: {}", name);
                    println!("  Scope: {}", scope);
                    if let Some(max) = max_uses {
                        println!("  Max uses: {}", max);
                    } else {
                        println!("  Max uses: unlimited");
                    }
                    println!("  Expires: {}", expires);
                    println!();
                    println!("  Deploy agents with:");
                    println!(
                        "    appcontrol-agent --enroll <gateway-url> --token {}",
                        token
                    );

                    EXIT_SUCCESS
                }
                Ok(resp) => {
                    eprintln!("Error: HTTP {}", resp.status());
                    EXIT_FAILURE
                }
                Err(e) => {
                    eprintln!("Connection error: {}", e);
                    EXIT_FAILURE
                }
            }
        }
        PkiCommands::ListTokens => {
            let url = format!("{}/api/v1/enrollment/tokens", base);

            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let body: Value = resp.json().await.unwrap_or_default();
                    let mut table = comfy_table::Table::new();
                    table.set_header(vec!["ID", "Name", "Scope", "Uses", "Expires", "Status"]);

                    if let Some(tokens) = body.get("tokens").and_then(|t| t.as_array()) {
                        for t in tokens {
                            let status = if t.get("revoked_at").and_then(|r| r.as_str()).is_some() {
                                "revoked"
                            } else {
                                "active"
                            };
                            let uses = format!(
                                "{}/{}",
                                t.get("current_uses").and_then(|u| u.as_i64()).unwrap_or(0),
                                t.get("max_uses")
                                    .and_then(|m| m.as_i64())
                                    .map(|m| m.to_string())
                                    .unwrap_or_else(|| "unlimited".to_string())
                            );
                            table.add_row(vec![
                                &t.get("id").and_then(|i| i.as_str()).unwrap_or("?")[..8],
                                t.get("name").and_then(|n| n.as_str()).unwrap_or("?"),
                                t.get("scope").and_then(|s| s.as_str()).unwrap_or("?"),
                                &uses,
                                t.get("expires_at").and_then(|e| e.as_str()).unwrap_or("?"),
                                status,
                            ]);
                        }
                    }
                    println!("{}", table);
                    EXIT_SUCCESS
                }
                _ => EXIT_FAILURE,
            }
        }
        PkiCommands::RevokeToken { id } => {
            let url = format!("{}/api/v1/enrollment/tokens/{}/revoke", base, id);

            match client.post(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    println!("Token {} revoked", id);
                    EXIT_SUCCESS
                }
                Ok(resp) if resp.status().as_u16() == 404 => {
                    eprintln!("Token not found: {}", id);
                    EXIT_NOT_FOUND
                }
                _ => EXIT_FAILURE,
            }
        }
        PkiCommands::IssueAgent {
            hostname,
            ca_cert,
            ca_key,
            out,
            validity_days,
        } => {
            let ca_cert_pem = match std::fs::read_to_string(&ca_cert) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to read CA cert {}: {}", ca_cert, e);
                    return EXIT_FAILURE;
                }
            };
            let ca_key_pem = match std::fs::read_to_string(&ca_key) {
                Ok(k) => k,
                Err(e) => {
                    eprintln!("Failed to read CA key {}: {}", ca_key, e);
                    return EXIT_FAILURE;
                }
            };

            match appcontrol_common::issue_agent_cert(
                &ca_cert_pem,
                &ca_key_pem,
                &hostname,
                validity_days,
            ) {
                Ok(issued) => {
                    let dir = std::path::Path::new(&out);
                    std::fs::create_dir_all(dir).ok();
                    let cert_path = dir.join("agent.crt");
                    let key_path = dir.join("agent.key");
                    let ca_path = dir.join("ca.crt");
                    std::fs::write(&cert_path, &issued.cert_pem).unwrap();
                    std::fs::write(&key_path, &issued.key_pem).unwrap();
                    std::fs::write(&ca_path, &issued.ca_pem).unwrap();

                    let fp =
                        appcontrol_common::fingerprint_pem(&issued.cert_pem).unwrap_or_default();
                    println!("  Agent certificate issued for: {}", hostname);
                    println!("  Fingerprint: {}", fp);
                    println!("  cert: {}", cert_path.display());
                    println!("  key:  {}", key_path.display());
                    println!("  ca:   {}", ca_path.display());
                    EXIT_SUCCESS
                }
                Err(e) => {
                    eprintln!("Failed to issue agent cert: {}", e);
                    EXIT_FAILURE
                }
            }
        }
        PkiCommands::IssueGateway {
            cn,
            san_dns,
            san_ip,
            ca_cert,
            ca_key,
            out,
            validity_days,
        } => {
            let ca_cert_pem = match std::fs::read_to_string(&ca_cert) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to read CA cert {}: {}", ca_cert, e);
                    return EXIT_FAILURE;
                }
            };
            let ca_key_pem = match std::fs::read_to_string(&ca_key) {
                Ok(k) => k,
                Err(e) => {
                    eprintln!("Failed to read CA key {}: {}", ca_key, e);
                    return EXIT_FAILURE;
                }
            };

            match appcontrol_common::issue_gateway_cert(
                &ca_cert_pem,
                &ca_key_pem,
                &cn,
                &san_dns,
                &san_ip,
                validity_days,
            ) {
                Ok(issued) => {
                    let dir = std::path::Path::new(&out);
                    std::fs::create_dir_all(dir).ok();
                    let cert_path = dir.join("gateway.crt");
                    let key_path = dir.join("gateway.key");
                    let ca_path = dir.join("ca.crt");
                    std::fs::write(&cert_path, &issued.cert_pem).unwrap();
                    std::fs::write(&key_path, &issued.key_pem).unwrap();
                    std::fs::write(&ca_path, &issued.ca_pem).unwrap();

                    let fp =
                        appcontrol_common::fingerprint_pem(&issued.cert_pem).unwrap_or_default();
                    println!("  Gateway certificate issued for: {}", cn);
                    println!("  Fingerprint: {}", fp);
                    println!("  cert: {}", cert_path.display());
                    println!("  key:  {}", key_path.display());
                    println!("  ca:   {}", ca_path.display());
                    EXIT_SUCCESS
                }
                Err(e) => {
                    eprintln!("Failed to issue gateway cert: {}", e);
                    EXIT_FAILURE
                }
            }
        }
        PkiCommands::Events => {
            let url = format!("{}/api/v1/enrollment/events", base);

            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let body: Value = resp.json().await.unwrap_or_default();
                    let mut table = comfy_table::Table::new();
                    table.set_header(vec!["Time", "Event", "Hostname", "IP", "Fingerprint"]);

                    if let Some(events) = body.get("events").and_then(|e| e.as_array()) {
                        for ev in events {
                            table.add_row(vec![
                                ev.get("created_at").and_then(|t| t.as_str()).unwrap_or("?"),
                                ev.get("event_type").and_then(|e| e.as_str()).unwrap_or("?"),
                                ev.get("hostname").and_then(|h| h.as_str()).unwrap_or("?"),
                                ev.get("ip_address").and_then(|i| i.as_str()).unwrap_or("?"),
                                ev.get("cert_fingerprint")
                                    .and_then(|f| f.as_str())
                                    .map(|f| &f[..std::cmp::min(f.len(), 16)])
                                    .unwrap_or("-"),
                            ]);
                        }
                    }
                    println!("{}", table);
                    EXIT_SUCCESS
                }
                _ => EXIT_FAILURE,
            }
        }
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
