//! Internal path construction. No public API accepts a relative store path.

use std::path::{Path, PathBuf};

use jingwei_jir::{ProjectId, RevisionId, SessionId};

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

    pub(crate) fn locks(&self) -> PathBuf {
        self.control().join("locks")
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
