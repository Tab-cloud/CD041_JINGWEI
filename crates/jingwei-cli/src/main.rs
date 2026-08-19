//! CLI that consumes JDP through the same authenticated client abstraction as other clients.

use std::{path::PathBuf, str::FromStr, sync::Arc};

use clap::{Parser, Subcommand, ValueEnum};
use jingwei_daemon::{Daemon, InMemoryClient};
use jingwei_jdp::{
    DiffInput, EventsInput, JDP_PROTOCOL_VERSION, JdpClient, JdpRequest, JdpResponse,
    PatchApplyInput, PatchInput, ProjectCreateInput, ProjectInfo, ProposalDecisionInput,
    ProposalDecisionOutput, QueryInput, ValidateInput, WorkspaceCapabilities, operation,
};
use jingwei_jir::{
    IdempotencyKey, ProjectId, ProposalAction, ProposalId, RequestId, RevisionId, WorkspaceId,
};
use jingwei_kernel::ExtensionRegistryBuilder;
use jingwei_store::WorkspaceStore;
use serde::{Serialize, de::DeserializeOwned};

#[derive(Parser)]
#[command(name = "jingwei", version, about = "Jingwei JDP command line client")]
struct Arguments {
    /// Project workspace directory.
    #[arg(long, global = true, default_value = ".")]
    workspace: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    Compile {
        file: PathBuf,
        #[arg(long)]
        project: ProjectId,
        #[arg(long)]
        base: Option<RevisionId>,
    },
    Query {
        selector: String,
        #[arg(long)]
        project: ProjectId,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    Patch {
        patch: PathBuf,
        #[arg(long)]
        project: ProjectId,
        #[arg(long)]
        base: RevisionId,
    },
    /// Accepts, rejects, or withdraws an immutable change proposal.
    Proposal {
        #[arg(long)]
        project: ProjectId,
        proposal: ProposalId,
        action: ProposalActionArgument,
        #[arg(long)]
        reason: Option<String>,
    },
    Diff {
        left: RevisionId,
        right: RevisionId,
        #[arg(long)]
        project: ProjectId,
    },
    Validate {
        revision: Option<RevisionId>,
        #[arg(long)]
        project: ProjectId,
    },
    Events {
        #[arg(long)]
        project: ProjectId,
        #[arg(long, default_value_t = 0)]
        after: u64,
    },
    /// Migrates an older workspace after creating a complete local backup.
    Migrate,
    Doctor,
}

#[derive(Clone, Copy, ValueEnum)]
enum ProposalActionArgument {
    Accept,
    Reject,
    Withdraw,
}

impl From<ProposalActionArgument> for ProposalAction {
    fn from(value: ProposalActionArgument) -> Self {
        match value {
            ProposalActionArgument::Accept => Self::Accept,
            ProposalActionArgument::Reject => Self::Reject,
            ProposalActionArgument::Withdraw => Self::Withdraw,
        }
    }
}

#[derive(Subcommand)]
enum ProjectCommand {
    Create {
        name: String,
        #[arg(long)]
        project_id: Option<ProjectId>,
    },
    Get {
        project: ProjectId,
    },
}

#[tokio::main]
async fn main() {
    if let Err(error) = run(Arguments::parse()).await {
        eprintln!("jingwei: {error}");
        std::process::exit(1);
    }
}

async fn run(arguments: Arguments) -> Result<(), Box<dyn std::error::Error>> {
    if matches!(&arguments.command, Command::Migrate) {
        println!(
            "{}",
            serde_json::to_string_pretty(&WorkspaceStore::migrate(&arguments.workspace)?)?
        );
        return Ok(());
    }

    let metadata_exists = arguments
        .workspace
        .join(".jingwei")
        .join("workspace.json")
        .is_file();
    let daemon = match (&arguments.command, metadata_exists) {
        (
            Command::Project {
                command: ProjectCommand::Create { .. },
            },
            false,
        ) => Arc::new(Daemon::create(
            &arguments.workspace,
            WorkspaceId::generate(),
            ExtensionRegistryBuilder::new().build(),
        )?),
        (_, true) => Arc::new(Daemon::open(
            &arguments.workspace,
            ExtensionRegistryBuilder::new().build(),
        )?),
        _ => {
            return Err(
                "workspace does not exist; run `jingwei project create <name>` first".into(),
            );
        }
    };
    let workspace_id = daemon.readiness().workspace_id;
    let client = InMemoryClient::connect(daemon, "jingwei-cli")?;
    match arguments.command {
        Command::Project {
            command: ProjectCommand::Create { name, project_id },
        } => {
            let project_id = project_id.unwrap_or_else(ProjectId::generate);
            print_response(
                send::<_, ProjectInfo>(
                    &client,
                    workspace_id,
                    project_id,
                    operation::PROJECT_CREATE,
                    None,
                    ProjectCreateInput { name },
                )
                .await?,
            )?;
        }
        Command::Project {
            command: ProjectCommand::Get { project },
        } => {
            print_response(
                send::<_, ProjectInfo>(
                    &client,
                    workspace_id,
                    project,
                    operation::PROJECT_GET,
                    None,
                    serde_json::json!({}),
                )
                .await?,
            )?;
        }
        Command::Compile {
            file,
            project,
            base,
        } => {
            let source = std::fs::read_to_string(&file)?;
            let base = match base {
                Some(base) => base,
                None => current_revision(&client, &workspace_id, &project).await?,
            };
            print_value_response(
                send::<_, serde_json::Value>(
                    &client,
                    workspace_id,
                    project,
                    operation::DESIGN_COMPILE_JDL,
                    Some(base),
                    jingwei_jdp::CompileJdlInput {
                        source_name: file.to_string_lossy().into_owned(),
                        source,
                    },
                )
                .await?,
            )?;
        }
        Command::Query {
            selector,
            project,
            limit,
        } => {
            let (kind, ids) = if let Some(kind) = selector.strip_prefix("kind=") {
                (Some(kind.to_owned()), Vec::new())
            } else if selector == "*" {
                (None, Vec::new())
            } else {
                (None, vec![jingwei_jir::EntityId::from_str(&selector)?])
            };
            print_value_response(
                send::<_, serde_json::Value>(
                    &client,
                    workspace_id,
                    project,
                    operation::DESIGN_QUERY,
                    None,
                    QueryInput {
                        revision: None,
                        kind,
                        ids,
                        limit,
                        cursor: None,
                    },
                )
                .await?,
            )?;
        }
        Command::Patch {
            patch,
            project,
            base,
        } => {
            let input: PatchInput = serde_json::from_slice(&std::fs::read(patch)?)?;
            print_value_response(
                send::<_, serde_json::Value>(
                    &client,
                    workspace_id,
                    project,
                    operation::DESIGN_PATCH_APPLY,
                    Some(base),
                    PatchApplyInput {
                        proposal_id: None,
                        patch: input,
                    },
                )
                .await?,
            )?;
        }
        Command::Proposal {
            project,
            proposal,
            action,
            reason,
        } => {
            print_response(
                send::<_, ProposalDecisionOutput>(
                    &client,
                    workspace_id,
                    project,
                    operation::CHANGE_PROPOSAL_DECIDE,
                    None,
                    ProposalDecisionInput {
                        proposal_id: proposal,
                        action: action.into(),
                        reason,
                    },
                )
                .await?,
            )?;
        }
        Command::Diff {
            left,
            right,
            project,
        } => {
            print_value_response(
                send::<_, serde_json::Value>(
                    &client,
                    workspace_id,
                    project,
                    operation::DESIGN_DIFF,
                    None,
                    DiffInput { left, right },
                )
                .await?,
            )?;
        }
        Command::Validate { revision, project } => {
            print_value_response(
                send::<_, serde_json::Value>(
                    &client,
                    workspace_id,
                    project,
                    operation::DESIGN_VALIDATE,
                    None,
                    ValidateInput {
                        revision,
                        scopes: Vec::new(),
                    },
                )
                .await?,
            )?;
        }
        Command::Events { project, after } => {
            print_value_response(
                send::<_, serde_json::Value>(
                    &client,
                    workspace_id,
                    project,
                    operation::TASK_EVENTS,
                    None,
                    EventsInput {
                        after_sequence: after,
                        limit: 1000,
                    },
                )
                .await?,
            )?;
        }
        Command::Migrate => unreachable!("migration exits before daemon startup"),
        Command::Doctor => {
            let project = ProjectId::new("workspace-capabilities")?;
            let response = send::<_, WorkspaceCapabilities>(
                &client,
                workspace_id,
                project,
                operation::WORKSPACE_CAPABILITIES,
                None,
                serde_json::json!({}),
            )
            .await?;
            print_response(response)?;
        }
    }
    Ok(())
}

async fn current_revision(
    client: &InMemoryClient,
    workspace: &WorkspaceId,
    project: &ProjectId,
) -> Result<RevisionId, Box<dyn std::error::Error>> {
    match send::<_, ProjectInfo>(
        client,
        workspace.clone(),
        project.clone(),
        operation::PROJECT_GET,
        None,
        serde_json::json!({}),
    )
    .await?
    {
        JdpResponse::Success(success) => Ok(success.data.current_revision),
        JdpResponse::Failure(failure) => Err(failure.error.summary.into()),
    }
}

async fn send<I, O>(
    client: &InMemoryClient,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    operation: &str,
    base_revision: Option<RevisionId>,
    input: I,
) -> Result<JdpResponse<O>, jingwei_jdp::ClientError>
where
    I: Serialize + Send + Sync,
    O: DeserializeOwned + Send,
{
    client
        .send(JdpRequest {
            protocol_version: JDP_PROTOCOL_VERSION.into(),
            request_id: RequestId::generate(),
            idempotency_key: IdempotencyKey::generate(),
            workspace_id,
            project_id,
            operation: operation.into(),
            execution_id: None,
            task_id: None,
            base_revision,
            deadline_ms: Some(30_000),
            dry_run: false,
            trace_context: None,
            input,
        })
        .await
}

fn print_response<T: Serialize>(response: JdpResponse<T>) -> Result<(), serde_json::Error> {
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

fn print_value_response(response: JdpResponse<serde_json::Value>) -> Result<(), serde_json::Error> {
    print_response(response)
}
