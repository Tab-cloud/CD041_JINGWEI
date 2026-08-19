#[cfg(windows)]
use std::{io::Read, io::Write, path::PathBuf, sync::Arc};

#[cfg(windows)]
use jingwei_daemon::{Daemon, production_registry, serve_named_pipe_once};
#[cfg(windows)]
use jingwei_jir::WorkspaceId;

#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let workspace_root = PathBuf::from(arguments.next().ok_or("missing workspace root")?);
    let pipe_name = arguments
        .next()
        .ok_or("missing pipe name")?
        .into_string()
        .map_err(|_| "pipe name is not UTF-8")?;
    let trace_output = PathBuf::from(arguments.next().ok_or("missing trace output path")?);
    let workspace_id = WorkspaceId::new("ws-typescript-rust-trace")?;

    let initial = Daemon::create(&workspace_root, workspace_id, production_registry()?)?;
    drop(initial);

    let mut token = String::new();
    std::io::stdin().read_to_string(&mut token)?;
    let daemon = Arc::new(Daemon::open_with_token(
        &workspace_root,
        production_registry()?,
        token.trim_end().to_owned(),
    )?);

    println!("READY");
    std::io::stdout().flush()?;
    serve_named_pipe_once(daemon.clone(), &pipe_name).await?;
    std::fs::write(
        trace_output,
        serde_json::to_vec_pretty(&daemon.trace_spans())?,
    )?;
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("trace_bridge_server is only available on Windows");
}
