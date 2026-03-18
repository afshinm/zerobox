#![allow(dead_code)]

mod api;
mod cli;
mod config;
mod guest;
mod image;
mod manager;
mod network;
mod snapshot;

use std::sync::Arc;

use clap::{Args, Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::api::AppState;
use crate::cli::DaemonClient;
use crate::manager::SandboxManager;
use crate::snapshot::SnapshotManager;

#[derive(Parser)]
#[command(
    name = "zerobox",
    about = "Firecracker sandbox supervisor for AI agents"
)]
struct Cli {
    /// Path to the configuration file
    #[arg(long, global = true, default_value = "/etc/zerobox/config.yaml")]
    config: String,

    /// Daemon endpoint URL (for CLI commands)
    #[arg(long, global = true, default_value = "http://localhost:7000")]
    endpoint: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the HTTP API server
    Serve,

    /// Create a new sandbox
    Start {
        /// Container/rootfs image to use
        #[arg(long)]
        image: Option<String>,

        /// Number of vCPUs
        #[arg(long)]
        vcpus: Option<u32>,

        /// Memory in MiB
        #[arg(long)]
        memory: Option<u32>,

        /// Timeout in milliseconds
        #[arg(long)]
        timeout: Option<u64>,

        /// Ports to expose (repeatable)
        #[arg(long, num_args = 1)]
        port: Vec<u16>,
    },

    /// Stop a running sandbox
    Stop {
        /// Sandbox ID
        id: String,
    },

    /// Destroy a sandbox
    Destroy {
        /// Sandbox ID
        id: String,
    },

    /// List all sandboxes
    List,

    /// Get details of a sandbox
    Get {
        /// Sandbox ID
        id: String,
    },

    /// Execute a command in a sandbox
    Exec {
        /// Sandbox ID
        id: String,

        /// Command and arguments to run
        #[arg(last = true)]
        cmd: Vec<String>,
    },

    /// Open an interactive shell in a sandbox
    Connect {
        /// Sandbox ID
        id: String,
    },

    /// Snapshot management commands
    Snapshot(SnapshotArgs),
}

#[derive(Args)]
struct SnapshotArgs {
    #[command(subcommand)]
    command: SnapshotCommands,
}

#[derive(Subcommand)]
enum SnapshotCommands {
    /// Create a snapshot from a sandbox
    Create {
        /// Sandbox ID to snapshot
        id: String,
    },

    /// Restore a sandbox from a snapshot
    Restore {
        /// Sandbox ID
        id: String,

        /// Snapshot ID to restore from
        snapshot_id: String,
    },

    /// List all snapshots
    List,

    /// Delete a snapshot
    Delete {
        /// Snapshot ID to delete
        snapshot_id: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if matches!(cli.command, Commands::Serve) {
        return run_server(&cli.config).await;
    }

    // All non-serve commands use the HTTP client
    let client = DaemonClient::new(&cli.endpoint);

    match cli.command {
        Commands::Serve => unreachable!(),
        Commands::Start {
            image,
            vcpus,
            memory,
            timeout,
            port,
        } => {
            let resp = client
                .create_sandbox(image.as_deref(), vcpus, memory, timeout, &port)
                .await?;
            let id = resp["sandboxId"].as_str().unwrap_or("unknown");
            let status = resp["status"].as_str().unwrap_or("unknown");
            if status == "failed" {
                let reason = resp["error"].as_str().unwrap_or("unknown error");
                eprintln!("Error: sandbox {} failed to start", id);
                eprintln!("  {}", reason);
                std::process::exit(1);
            }
            println!("{} (status: {})", id, status);
        }
        Commands::Stop { id } => {
            let resp = client.stop_sandbox(&id).await?;
            let status = resp["status"].as_str().unwrap_or("unknown");
            println!("Stopped {} (status: {})", id, status);
        }
        Commands::Destroy { id } => {
            client.destroy_sandbox(&id).await?;
            println!("Destroyed {}", id);
        }
        Commands::List => {
            let resp = client.list_sandboxes().await?;
            if let Some(sandboxes) = resp["sandboxes"].as_array() {
                if sandboxes.is_empty() {
                    println!("No sandboxes");
                } else {
                    println!("{:<20} {:<10} {:<16} CREATED", "ID", "STATUS", "IP");
                    for s in sandboxes {
                        println!(
                            "{:<20} {:<10} {:<16} {}",
                            s["sandboxId"].as_str().unwrap_or("-"),
                            s["status"].as_str().unwrap_or("-"),
                            s["ip"].as_str().unwrap_or("-"),
                            s["createdAt"].as_str().unwrap_or("-"),
                        );
                    }
                }
            }
        }
        Commands::Get { id } => {
            let resp = client.get_sandbox(&id).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
        Commands::Exec { id, cmd } => {
            if cmd.is_empty() {
                return Err(anyhow::anyhow!(
                    "No command specified. Usage: zerobox exec <id> -- <cmd> [args...]"
                ));
            }
            let resp = client.exec_command(&id, &cmd[0], &cmd[1..]).await?;
            // Print stdout/stderr
            if let Some(stdout) = resp["stdout"].as_str() {
                if !stdout.is_empty() {
                    print!("{}", stdout);
                }
            }
            if let Some(stderr) = resp["stderr"].as_str() {
                if !stderr.is_empty() {
                    eprint!("{}", stderr);
                }
            }
            // Exit with the command's exit code
            if let Some(exit_code) = resp["exitCode"].as_i64() {
                if exit_code != 0 {
                    std::process::exit(exit_code as i32);
                }
            }
        }
        Commands::Connect { id } => {
            // Verify sandbox exists and is running
            let resp = client.get_sandbox(&id).await?;
            let status = resp["status"].as_str().unwrap_or("unknown");
            if status != "running" {
                return Err(anyhow::anyhow!(
                    "Sandbox {} is not running (status: {})",
                    id,
                    status
                ));
            }

            eprintln!(
                "Connected to sandbox {}. Type commands, Ctrl-D to exit.",
                id
            );
            eprintln!("---");

            let stdin = std::io::stdin();
            let mut line = String::new();
            loop {
                // Print prompt
                eprint!("zerobox:{}$ ", &id[4..id.len().min(12)]);

                line.clear();
                let n = std::io::BufRead::read_line(&mut stdin.lock(), &mut line)?;
                if n == 0 {
                    // EOF (Ctrl-D)
                    eprintln!("\nDisconnected.");
                    break;
                }

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Parse as shell: first word is cmd, rest are args
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                let cmd_name = parts[0];
                let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

                match client.exec_command(&id, cmd_name, &args).await {
                    Ok(resp) => {
                        if let Some(stdout) = resp["stdout"].as_str() {
                            if !stdout.is_empty() {
                                print!("{}", stdout);
                            }
                        }
                        if let Some(stderr) = resp["stderr"].as_str() {
                            if !stderr.is_empty() {
                                eprint!("{}", stderr);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }
        }
        Commands::Snapshot(args) => match args.command {
            SnapshotCommands::Create { id } => {
                let resp = client.create_snapshot(&id).await?;
                let snap_id = resp["snapshotId"].as_str().unwrap_or("unknown");
                println!("Created snapshot {} from sandbox {}", snap_id, id);
            }
            SnapshotCommands::Restore { id, snapshot_id } => {
                // Restore is not yet implemented in the API
                println!(
                    "Restore not yet implemented (sandbox={}, snapshot={})",
                    id, snapshot_id
                );
            }
            SnapshotCommands::List => {
                let resp = client.list_snapshots().await?;
                if let Some(snapshots) = resp.as_array() {
                    if snapshots.is_empty() {
                        println!("No snapshots");
                    } else {
                        println!("{:<20} {:<20} {:<10} CREATED", "ID", "SOURCE", "STATUS");
                        for s in snapshots {
                            println!(
                                "{:<20} {:<20} {:<10} {}",
                                s["snapshotId"].as_str().unwrap_or("-"),
                                s["sourceSandboxId"].as_str().unwrap_or("-"),
                                s["status"].as_str().unwrap_or("-"),
                                s["createdAt"].as_str().unwrap_or("-"),
                            );
                        }
                    }
                }
            }
            SnapshotCommands::Delete { snapshot_id } => {
                client.delete_snapshot(&snapshot_id).await?;
                println!("Deleted snapshot {}", snapshot_id);
            }
        },
    }

    Ok(())
}

async fn run_server(config_path: &str) -> anyhow::Result<()> {
    let config = config::load(config_path)?;

    // Initialize tracing
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    tracing::info!("Starting zerobox daemon");

    let config = Arc::new(config);

    // Set up networking
    let network_manager = network::NetworkManager::new(config.networking.clone());
    if let Err(e) = network_manager.setup_bridge().await {
        tracing::warn!("Failed to setup bridge (may already exist): {}", e);
    }
    // Determine outbound interface for NAT
    let outbound_iface = if config.networking.outbound_interface == "auto" {
        // Auto-detect by finding the default route interface
        match network::nat::detect_outbound_interface().await {
            Ok(iface) => {
                tracing::info!("Auto-detected outbound interface: {}", iface);
                Some(iface)
            }
            Err(e) => {
                tracing::warn!("Failed to auto-detect outbound interface: {}", e);
                None
            }
        }
    } else {
        Some(config.networking.outbound_interface.clone())
    };

    if let Some(iface) = outbound_iface {
        if let Err(e) = network::nat::setup_nat(&iface, &config.networking.subnet).await {
            tracing::warn!("Failed to setup NAT: {}", e);
        }
    }

    let sandbox_manager = Arc::new(SandboxManager::new(config.clone(), network_manager));
    let snapshot_manager = Arc::new(SnapshotManager::new(config.clone()));

    let state = AppState {
        config: config.clone(),
        sandbox_manager,
        snapshot_manager,
    };

    let app = api::router(state);

    let listener = tokio::net::TcpListener::bind(&config.listen).await?;
    tracing::info!("Listening on {}", config.listen);

    axum::serve(listener, app).await?;

    Ok(())
}
