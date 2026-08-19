//! Internal path construction. No public API accepts a relative store path.

use std::path::{Path, PathBuf};

use jingwei_jir::{ExecutionId, ProjectId, ProposalId, RevisionId, SessionId, TaskId};

#[derive(Clone, Debug)]
pub(crate) struct Layout {
    root: PathBuf,
}

impl Layout {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn control(&self) -> PathBuf {
        self.root.join(".jingwei")
    }

    pub(crate) fn workspace_metadata(&self) -> PathBuf {
        self.control().join("workspace.json")
    }

    pub(crate) fn projects(&self) -> PathBuf {
        self.control().join("projects")
    }

    pub(crate) fn project(&self, project: &ProjectId) -> PathBuf {
        self.projects().join(safe_component(project.as_str()))
    }

    pub(crate) fn project_metadata(&self, project: &ProjectId) -> PathBuf {
        self.project(project).join("project.json")
    }

    pub(crate) fn revisions(&self, project: &ProjectId) -> PathBuf {
        self.project(project).join("revisions")
    }

    pub(crate) fn revision(&self, project: &ProjectId, revision: &RevisionId) -> PathBuf {
        self.revisions(project)
            .join(format!("{}.json", safe_component(revision.as_str())))
    }

    pub(crate) fn journal(&self, project: &ProjectId) -> PathBuf {
        self.project(project).join("journal")
    }

    pub(crate) fn commits(&self, project: &ProjectId) -> PathBuf {
        self.project(project).join("commits")
    }

    pub(crate) fn proposals(&self, project: &ProjectId) -> PathBuf {
        self.project(project).join("proposals")
    }

    pub(crate) fn proposal(&self, project: &ProjectId, proposal: &ProposalId) -> PathBuf {
        self.proposals(project)
            .join(format!("{}.json", safe_component(proposal.as_str())))
    }

    pub(crate) fn proposal_decisions(&self, project: &ProjectId, proposal: &ProposalId) -> PathBuf {
        self.proposals(project)
            .join(format!("{}.decisions", safe_component(proposal.as_str())))
    }

    pub(crate) fn idempotency(&self, project: &ProjectId) -> PathBuf {
        self.project(project).join("idempotency")
    }

    pub(crate) fn current_pointers(&self, project: &ProjectId) -> PathBuf {
        self.project(project).join("pointers")
    }

    pub(crate) fn draft(&self, project: &ProjectId, session: &SessionId) -> PathBuf {
        self.project(project)
            .join("drafts")
            .join(safe_component(session.as_str()))
    }

    pub(crate) fn draft_pointers(&self, project: &ProjectId, session: &SessionId) -> PathBuf {
        self.draft(project, session).join("pointers")
    }

    pub(crate) fn draft_metadata(&self, project: &ProjectId, session: &SessionId) -> PathBuf {
        self.draft(project, session).join("metadata")
    }

    pub(crate) fn transactions(&self) -> PathBuf {
        self.control().join("transactions")
    }

    pub(crate) fn quarantine(&self) -> PathBuf {
        self.control().join("quarantine")
    }

    pub(crate) fn executions(&self) -> PathBuf {
        self.control().join("executions")
    }

    pub(crate) fn execution(&self, execution: &ExecutionId) -> PathBuf {
        self.executions()
            .join(format!("{}.json", safe_component(execution.as_str())))
    }

    pub(crate) fn tasks(&self) -> PathBuf {
        self.control().join("tasks")
    }

    pub(crate) fn task(&self, task: &TaskId) -> PathBuf {
        self.tasks()
            .join(format!("{}.json", safe_component(task.as_str())))
    }

    pub(crate) fn locks(&self) -> PathBuf {
        self.control().join("locks")
    }

    pub(crate) fn daemon_epoch(&self) -> PathBuf {
        self.control().join("daemon-epoch.json")
    }

    pub(crate) fn events(&self, project: &ProjectId) -> PathBuf {
        self.project(project).join("events")
    }

    pub(crate) fn artifacts(&self) -> PathBuf {
        self.control().join("artifacts").join("sha256")
    }
}

fn safe_component(value: &str) -> String {
    debug_assert!(!value.is_empty());
    hex::encode(value.as_bytes())
}
