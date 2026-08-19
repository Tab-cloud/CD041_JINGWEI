//! Minimal sidecar entrypoint. The bootstrap token is read from inherited stdin, never argv or logs.

use std::{io::Read, path::PathBuf, sync::Arc};

use clap::{Parser, Subcommand};
use jingwei_kernel::ExtensionRegistryBuilder;

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
        #[arg(long)]
        pipe_name: String,
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
                jingwei_daemon::Daemon::open(workspace, ExtensionRegistryBuilder::new().build())?;
            println!("{}", serde_json::to_string_pretty(&daemon.readiness())?);
        }
        #[cfg(windows)]
        Command::Serve {
            workspace,
            pipe_name,
        } => {
            let mut token = String::new();
            std::io::stdin().take(4096).read_to_string(&mut token)?;
            let token = token.trim().to_owned();
            let daemon = Arc::new(jingwei_daemon::Daemon::open_with_token(
                workspace,
                ExtensionRegistryBuilder::new().build(),
                token,
            )?);
            loop {
                jingwei_daemon::serve_named_pipe_once(daemon.clone(), &pipe_name).await?;
            }
        }
    }
    Ok(())
}
