//! Minimal sidecar entrypoint. The bootstrap token is read from inherited stdin, never argv or logs.

use std::{
    io::{BufRead, Write},
    path::PathBuf,
    sync::Arc,
};

use clap::{Parser, Subcommand};
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "jingwei-daemon",
    version,
    about = "Jingwei single-writer JDP daemon"
)]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check store recovery/readiness and exit.
    Doctor {
        #[arg(long)]
        workspace: PathBuf,
    },
    /// Serve authenticated JDP on a Windows named pipe.
    #[cfg(windows)]
    Serve {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long, default_value = "ws-desktop")]
        workspace_id: String,
        #[arg(long)]
        pipe_name: String,
        /// Explicitly migrate an older supported workspace before serving it.
        #[arg(long)]
        migrate_if_needed: bool,
    },
}

#[tokio::main]
async fn main() {
    let result = run(Arguments::parse()).await;
    if let Err(error) = result {
        eprintln!("jingwei-daemon: {error}");
        std::process::exit(1);
    }
}

async fn run(arguments: Arguments) -> Result<(), Box<dyn std::error::Error>> {
    match arguments.command {
        Command::Doctor { workspace } => {
            let daemon =
                jingwei_daemon::Daemon::open(workspace, jingwei_daemon::production_registry()?)?;
            println!("{}", serde_json::to_string_pretty(&daemon.readiness())?);
        }
        #[cfg(windows)]
        Command::Serve {
            workspace,
            workspace_id,
            pipe_name,
            migrate_if_needed,
        } => {
            let mut token = String::new();
            std::io::stdin().lock().read_line(&mut token)?;
            let token = token.trim().to_owned();
            let metadata = workspace.join(".jingwei").join("workspace.json");
            if metadata.is_file() && migrate_if_needed {
                match jingwei_store::WorkspaceStore::migrate(&workspace) {
                    Ok(report) => eprintln!(
                        "jingwei-daemon: migrated workspace schema {} -> {} with backup {}",
                        report.source_version,
                        report.target_version,
                        report.backup_path.display()
                    ),
                    Err(jingwei_store::StoreError::MigrationNotRequired(_)) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            let daemon = Arc::new(if metadata.is_file() {
                jingwei_daemon::Daemon::open_with_token(
                    &workspace,
                    jingwei_daemon::production_registry()?,
                    token,
                )?
            } else {
                jingwei_daemon::Daemon::create_with_token(
                    &workspace,
                    jingwei_jir::WorkspaceId::new(workspace_id)?,
                    jingwei_daemon::production_registry()?,
                    token,
                )?
            });
            println!(
                "{}",
                serde_json::to_string(&SidecarHandshake {
                    protocol_version: jingwei_jdp::JDP_PROTOCOL_VERSION,
                    version: env!("CARGO_PKG_VERSION"),
                    target: current_target(),
                    health: "healthy",
                })?
            );
            std::io::stdout().flush()?;
            jingwei_daemon::serve_named_pipe(daemon, &pipe_name).await?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SidecarHandshake<'a> {
    protocol_version: &'a str,
    version: &'a str,
    target: &'a str,
    health: &'a str,
}

fn current_target() -> &'static str {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("aarch64", "windows") => "aarch64-pc-windows-msvc",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        _ => "unsupported-target",
    }
}
