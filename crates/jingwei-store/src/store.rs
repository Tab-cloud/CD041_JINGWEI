//! Workspace store implementation and recovery protocol.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use jingwei_jir::{
    ArtifactId, ExecutionId, IdempotencyKey, ProjectId, RequestId, RevisionId, RevisionSnapshot,
    SessionId, WorkspaceId,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    ArtifactRecord, MAX_ARTIFACT_BYTES, STORE_SCHEMA_VERSION, WriterLease, layout::Layout,
};

/// Workspace-level immutable identity and storage format metadata.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMetadata {
    pub workspace_id: WorkspaceId,
    pub schema_version: u32,
    pub created_at: DateTime<Utc>,
}

/// Project identity. Current revision is derived from append-only pointer records.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMetadata {
    pub project_id: ProjectId,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// Immutable operation journal entry.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationJournalRecord {
    pub execution_id: ExecutionId,
    pub request_id: RequestId,
    pub project_id: ProjectId,
    pub idempotency_key: IdempotencyKey,
    pub operation: String,
    pub base_revision: Option<RevisionId>,
    pub result_revision: RevisionId,
    pub normalized_input_hash: String,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub operations: Value,
    #[serde(default)]
    pub artifact_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

/// Durable replay result for an idempotency key.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdempotencyRecord {
    pub key: IdempotencyKey,
    pub project_id: ProjectId,
    pub base_revision: Option<RevisionId>,
    pub normalized_input_hash: String,
    pub result_revision: RevisionId,
    pub response: Value,
}

/// Durable event with aggregate-local monotonically increasing sequence.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredEvent {
    pub aggregate_id: String,
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub data: Value,
}

/// Logical pointer moved by a commit.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "sessionId")]
pub enum PointerTarget {
    ProjectCurrent,
    Draft(SessionId),
    None,
}

/// Validated inputs to one atomic store transaction.
pub struct CommitRequest {
    pub snapshot: RevisionSnapshot,
    pub journal: OperationJournalRecord,
    pub idempotency: IdempotencyRecord,
    pub event: StoredEvent,
    pub pointer_target: PointerTarget,
    /// Pointer value expected before the transaction, distinct from a new draft's semantic parent.
    pub expected_pointer: Option<RevisionId>,
}

/// Failure injection boundary used by recovery tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrashPoint {
    AfterPrepare,
    AfterRevision,
    AfterJournal,
    AfterPointer,
}

/// Startup reconciliation result.
#[derive(Clone, Debug, Default)]
pub struct RecoveryReport {
    pub recovered_transactions: usize,
    pub quarantined_transactions: usize,
    pub removed_incomplete_transactions: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct WriterOwner {
    instance_id: String,
    process_id: u32,
    acquired_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PointerRecord {
    sequence: u64,
    revision_id: RevisionId,
    transaction_id: String,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransactionManifest {
    transaction_id: String,
    project_id: ProjectId,
    expected_pointer: Option<RevisionId>,
    result_revision: RevisionId,
    pointer_target: PointerTarget,
    pointer_sequence: Option<u64>,
    journal_file: String,
    idempotency_file: String,
    event_file: String,
}

/// Open local workspace store. Clone is safe; writes still require an external `WriterLease`.
#[derive(Clone, Debug)]
pub struct WorkspaceStore {
    layout: Layout,
    metadata: WorkspaceMetadata,
    read_only_reason: Option<String>,
}

impl WorkspaceStore {
    /// Creates a new workspace, or opens an existing one with the same identity.
    pub fn create(root: impl AsRef<Path>, workspace_id: WorkspaceId) -> Result<Self, StoreError> {
        let root = absolute_normalized(root.as_ref())?;
        fs::create_dir_all(&root)?;
        let layout = Layout::new(root);
        create_layout(&layout)?;
        let metadata_path = layout.workspace_metadata();
        if metadata_path.exists() {
            let existing: WorkspaceMetadata = read_json(&metadata_path)?;
            if existing.workspace_id != workspace_id {
                return Err(StoreError::WorkspaceIdentityMismatch);
            }
            return Self::open(layout.root());
        }
        let metadata = WorkspaceMetadata {
            workspace_id,
            schema_version: STORE_SCHEMA_VERSION,
            created_at: Utc::now(),
        };
        write_new_json(&metadata_path, &metadata)?;
        Ok(Self {
            layout,
            metadata,
            read_only_reason: None,
        })
    }

    /// Opens an existing workspace. A newer store schema is opened read-only.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let root = absolute_normalized(root.as_ref())?;
        let layout = Layout::new(root);
        let metadata: WorkspaceMetadata = read_json(&layout.workspace_metadata())?;
        let read_only_reason = (metadata.schema_version > STORE_SCHEMA_VERSION).then(|| {
            format!(
                "workspace schema {} is newer than supported {}",
                metadata.schema_version, STORE_SCHEMA_VERSION
            )
        });
        Ok(Self {
            layout,
            metadata,
            read_only_reason,
        })
    }

    /// Returns immutable workspace metadata.
    pub const fn metadata(&self) -> &WorkspaceMetadata {
        &self.metadata
    }

    /// Returns the reason writes are disabled, if any.
    pub fn read_only_reason(&self) -> Option<&str> {
        self.read_only_reason.as_deref()
    }

    /// Acquires the process-level exclusive writer lease.
    pub fn acquire_writer(
        &self,
        instance_id: impl Into<String>,
    ) -> Result<WriterLease, StoreError> {
        self.ensure_writable()?;
        fs::create_dir_all(self.layout.locks())?;
        let owner_path = self.layout.locks().join("writer.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&owner_path)?;
        let owner = WriterOwner {
            instance_id: instance_id.into(),
            process_id: std::process::id(),
            acquired_at: Utc::now(),
        };
        WriterLease::acquire(file, owner_path, &owner)
    }

    /// Creates immutable project metadata and its subdirectories.
    pub fn create_project(&self, metadata: &ProjectMetadata) -> Result<(), StoreError> {
        self.ensure_writable()?;
        validate_project_name(&metadata.name)?;
        let root = self.layout.project(&metadata.project_id);
        fs::create_dir_all(&root)?;
        for path in [
            self.layout.revisions(&metadata.project_id),
            self.layout.journal(&metadata.project_id),
            self.layout.idempotency(&metadata.project_id),
            self.layout.current_pointers(&metadata.project_id),
            self.layout.events(&metadata.project_id),
            root.join("drafts"),
        ] {
            fs::create_dir_all(path)?;
        }
        write_new_json(
            &self.layout.project_metadata(&metadata.project_id),
            metadata,
        )
    }

    /// Reads project metadata.
    pub fn project(&self, project: &ProjectId) -> Result<ProjectMetadata, StoreError> {
        read_json(&self.layout.project_metadata(project))
    }

    /// Returns whether immutable project metadata exists.
    pub fn project_exists(&self, project: &ProjectId) -> bool {
        self.layout.project_metadata(project).is_file()
    }

    /// Returns the next aggregate-local durable event sequence.
    pub fn next_event_sequence(&self, project: &ProjectId) -> Result<u64, StoreError> {
        let Some(path) = sorted_files(&self.layout.events(project))?.pop() else {
            return Ok(1);
        };
        let event: StoredEvent = read_json(&path)?;
        Ok(event.sequence + 1)
    }

    /// Appends durable draft metadata without overwriting an earlier checkpoint.
    pub fn append_draft_metadata<T: Serialize>(
        &self,
        project: &ProjectId,
        session: &SessionId,
        value: &T,
    ) -> Result<(), StoreError> {
        self.ensure_writable()?;
        let directory = self.layout.draft_metadata(project, session);
        fs::create_dir_all(&directory)?;
        let sequence = next_file_sequence(&directory)?;
        write_new_json(&directory.join(format!("{sequence:020}.json")), value)
    }

    /// Reads the newest durable draft metadata checkpoint.
    pub fn draft_metadata<T: DeserializeOwned>(
        &self,
        project: &ProjectId,
        session: &SessionId,
    ) -> Result<T, StoreError> {
        let directory = self.layout.draft_metadata(project, session);
        let path = sorted_files(&directory)?
            .pop()
            .ok_or_else(|| StoreError::DraftNotFound(session.clone()))?;
        read_json(&path)
    }

    /// Returns the current project revision from the last valid append-only pointer.
    pub fn current_revision(&self, project: &ProjectId) -> Result<Option<RevisionId>, StoreError> {
        latest_pointer(&self.layout.current_pointers(project))
            .map(|pointer| pointer.map(|pointer| pointer.revision_id))
    }

    /// Returns the current draft head.
    pub fn draft_revision(
        &self,
        project: &ProjectId,
        session: &SessionId,
    ) -> Result<Option<RevisionId>, StoreError> {
        latest_pointer(&self.layout.draft_pointers(project, session))
            .map(|pointer| pointer.map(|pointer| pointer.revision_id))
    }

    /// Loads and verifies one immutable snapshot.
    pub fn revision(
        &self,
        project: &ProjectId,
        revision: &RevisionId,
    ) -> Result<RevisionSnapshot, StoreError> {
        let snapshot: RevisionSnapshot = read_json(&self.layout.revision(project, revision))?;
        snapshot.verify()?;
        Ok(snapshot)
    }

    /// Looks up a completed request by idempotency key.
    pub fn idempotency(
        &self,
        project: &ProjectId,
        key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyRecord>, StoreError> {
        let path = self
            .layout
            .idempotency(project)
            .join(format!("{}.json", key_hash(key)));
        if path.exists() {
            read_json(&path).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Commits a prepared transaction. Pointer creation is the final authoritative step.
    pub fn commit(&self, request: CommitRequest) -> Result<(), StoreError> {
        self.commit_with_crash(request, None)
    }

    /// Same as `commit`, with an optional deterministic crash boundary for tests.
    pub fn commit_with_crash(
        &self,
        request: CommitRequest,
        crash: Option<CrashPoint>,
    ) -> Result<(), StoreError> {
        self.ensure_writable()?;
        validate_commit(&request)?;
        let project = &request.snapshot.metadata.project_id;
        let current = self.pointer_revision(project, &request.pointer_target)?;
        if current != request.expected_pointer {
            return Err(StoreError::StaleBase {
                expected: request.expected_pointer,
                actual: current,
            });
        }
        let pointer_sequence = match request.pointer_target {
            PointerTarget::ProjectCurrent => Some(next_pointer_sequence(
                &self.layout.current_pointers(project),
            )?),
            PointerTarget::Draft(ref session) => {
                let path = self.layout.draft_pointers(project, session);
                fs::create_dir_all(&path)?;
                Some(next_pointer_sequence(&path)?)
            }
            PointerTarget::None => None,
        };
        let transaction_id = format!("txn-{}", Uuid::new_v4());
        let transaction = self.layout.transactions().join(&transaction_id);
        fs::create_dir(&transaction)?;
        let journal_file = format!("{}.json", request.journal.execution_id);
        let idempotency_file = format!("{}.json", key_hash(&request.idempotency.key));
        let event_file = format!("{:020}.json", request.event.sequence);
        write_new_json(&transaction.join("revision.json"), &request.snapshot)?;
        write_new_json(&transaction.join("journal.json"), &request.journal)?;
        write_new_json(&transaction.join("idempotency.json"), &request.idempotency)?;
        write_new_json(&transaction.join("event.json"), &request.event)?;
        let manifest = TransactionManifest {
            transaction_id: transaction_id.clone(),
            project_id: project.clone(),
            expected_pointer: request.expected_pointer,
            result_revision: request.snapshot.metadata.id.clone(),
            pointer_target: request.pointer_target,
            pointer_sequence,
            journal_file,
            idempotency_file,
            event_file,
        };
        write_new_json(&transaction.join("manifest.json"), &manifest)?;
        maybe_crash(crash, CrashPoint::AfterPrepare)?;
        self.install_revision(&transaction, &manifest)?;
        maybe_crash(crash, CrashPoint::AfterRevision)?;
        self.install_journal_and_ledger(&transaction, &manifest)?;
        maybe_crash(crash, CrashPoint::AfterJournal)?;
        self.install_pointer(&manifest)?;
        maybe_crash(crash, CrashPoint::AfterPointer)?;
        fs::remove_dir_all(transaction)?;
        Ok(())
    }

    /// Reconciles every transaction with a durable manifest.
    pub fn recover(&self) -> Result<RecoveryReport, StoreError> {
        self.ensure_writable()?;
        let mut report = RecoveryReport::default();
        for entry in fs::read_dir(self.layout.transactions())? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let transaction = entry.path();
            let manifest_path = transaction.join("manifest.json");
            if !manifest_path.exists() {
                fs::remove_dir_all(transaction)?;
                report.removed_incomplete_transactions += 1;
                continue;
            }
            let manifest: TransactionManifest = read_json(&manifest_path)?;
            let current = self.pointer_revision(&manifest.project_id, &manifest.pointer_target)?;
            if current != manifest.expected_pointer
                && current != Some(manifest.result_revision.clone())
            {
                let quarantine = self.layout.quarantine().join(&manifest.transaction_id);
                fs::rename(&transaction, quarantine)?;
                report.quarantined_transactions += 1;
                continue;
            }
            self.install_revision(&transaction, &manifest)?;
            self.install_journal_and_ledger(&transaction, &manifest)?;
            self.install_pointer(&manifest)?;
            fs::remove_dir_all(transaction)?;
            report.recovered_transactions += 1;
        }
        Ok(report)
    }

    /// Stores bounded bytes in the workspace content-addressed store.
    pub fn put_artifact(
        &self,
        bytes: &[u8],
        media_type: impl Into<String>,
    ) -> Result<ArtifactRecord, StoreError> {
        self.ensure_writable()?;
        if bytes.len() > MAX_ARTIFACT_BYTES {
            return Err(StoreError::ArtifactTooLarge(bytes.len()));
        }
        let sha256 = hex::encode(Sha256::digest(bytes));
        let directory = self.layout.artifacts().join(&sha256[..2]);
        fs::create_dir_all(&directory)?;
        let data_path = directory.join(format!("{sha256}.bin"));
        if !data_path.exists() {
            write_new_bytes(&data_path, bytes)?;
        }
        let record = ArtifactRecord {
            id: ArtifactId::new(format!("artifact-{sha256}")).expect("generated ID is valid"),
            sha256: sha256.clone(),
            byte_length: bytes.len() as u64,
            media_type: media_type.into(),
            created_at: Utc::now(),
        };
        let metadata_path = directory.join(format!("{sha256}.json"));
        if !metadata_path.exists() {
            write_new_json(&metadata_path, &record)?;
        }
        Ok(record)
    }

    /// Reads content-addressed bytes and verifies their digest.
    pub fn artifact(&self, id: &ArtifactId) -> Result<(ArtifactRecord, Vec<u8>), StoreError> {
        let sha256 = id
            .as_str()
            .strip_prefix("artifact-")
            .ok_or(StoreError::InvalidArtifactId)?;
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(StoreError::InvalidArtifactId);
        }
        let directory = self.layout.artifacts().join(&sha256[..2]);
        let record: ArtifactRecord = read_json(&directory.join(format!("{sha256}.json")))?;
        let mut bytes = Vec::new();
        File::open(directory.join(format!("{sha256}.bin")))?.read_to_end(&mut bytes)?;
        if hex::encode(Sha256::digest(&bytes)) != sha256 {
            return Err(StoreError::ArtifactHashMismatch);
        }
        Ok((record, bytes))
    }

    /// Replays durable events after an aggregate-local sequence.
    pub fn events_after(
        &self,
        project: &ProjectId,
        after: u64,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, StoreError> {
        let mut events = Vec::new();
        for entry in sorted_files(&self.layout.events(project))? {
            let event: StoredEvent = read_json(&entry)?;
            if event.sequence > after {
                events.push(event);
                if events.len() >= limit.min(1000) {
                    break;
                }
            }
        }
        Ok(events)
    }

    fn ensure_writable(&self) -> Result<(), StoreError> {
        if let Some(reason) = &self.read_only_reason {
            Err(StoreError::ReadOnly(reason.clone()))
        } else {
            Ok(())
        }
    }

    fn pointer_revision(
        &self,
        project: &ProjectId,
        target: &PointerTarget,
    ) -> Result<Option<RevisionId>, StoreError> {
        match target {
            PointerTarget::ProjectCurrent => self.current_revision(project),
            PointerTarget::Draft(session) => self.draft_revision(project, session),
            PointerTarget::None => Ok(None),
        }
    }

    fn install_revision(
        &self,
        transaction: &Path,
        manifest: &TransactionManifest,
    ) -> Result<(), StoreError> {
        install_immutable(
            &transaction.join("revision.json"),
            &self
                .layout
                .revision(&manifest.project_id, &manifest.result_revision),
        )
    }

    fn install_journal_and_ledger(
        &self,
        transaction: &Path,
        manifest: &TransactionManifest,
    ) -> Result<(), StoreError> {
        install_immutable(
            &transaction.join("journal.json"),
            &self
                .layout
                .journal(&manifest.project_id)
                .join(&manifest.journal_file),
        )?;
        install_immutable(
            &transaction.join("idempotency.json"),
            &self
                .layout
                .idempotency(&manifest.project_id)
                .join(&manifest.idempotency_file),
        )?;
        install_immutable(
            &transaction.join("event.json"),
            &self
                .layout
                .events(&manifest.project_id)
                .join(&manifest.event_file),
        )
    }

    fn install_pointer(&self, manifest: &TransactionManifest) -> Result<(), StoreError> {
        let Some(sequence) = manifest.pointer_sequence else {
            return Ok(());
        };
        let directory = match &manifest.pointer_target {
            PointerTarget::ProjectCurrent => self.layout.current_pointers(&manifest.project_id),
            PointerTarget::Draft(session) => {
                self.layout.draft_pointers(&manifest.project_id, session)
            }
            PointerTarget::None => return Ok(()),
        };
        fs::create_dir_all(&directory)?;
        let path = directory.join(format!("{sequence:020}.json"));
        if path.exists() {
            let existing: PointerRecord = read_json(&path)?;
            if existing.revision_id == manifest.result_revision {
                return Ok(());
            }
            return Err(StoreError::PointerConflict);
        }
        let pointer = PointerRecord {
            sequence,
            revision_id: manifest.result_revision.clone(),
            transaction_id: manifest.transaction_id.clone(),
            created_at: Utc::now(),
        };
        write_new_json(&path, &pointer)
    }
}

fn create_layout(layout: &Layout) -> Result<(), StoreError> {
    for path in [
        layout.control(),
        layout.projects(),
        layout.transactions(),
        layout.quarantine(),
        layout.locks(),
        layout.artifacts(),
    ] {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

fn absolute_normalized(path: &Path) -> Result<PathBuf, StoreError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(absolute.components().collect())
}

fn validate_project_name(name: &str) -> Result<(), StoreError> {
    if name.trim().is_empty() || name.len() > 200 || name.chars().any(char::is_control) {
        Err(StoreError::InvalidProjectName)
    } else {
        Ok(())
    }
}

fn validate_commit(request: &CommitRequest) -> Result<(), StoreError> {
    request.snapshot.verify()?;
    if request.snapshot.metadata.id != request.journal.result_revision
        || request.snapshot.metadata.id != request.idempotency.result_revision
        || request.snapshot.metadata.project_id != request.journal.project_id
        || request.snapshot.metadata.project_id != request.idempotency.project_id
        || request.journal.idempotency_key != request.idempotency.key
        || request.journal.normalized_input_hash != request.idempotency.normalized_input_hash
    {
        return Err(StoreError::CommitInvariant);
    }
    Ok(())
}

fn maybe_crash(actual: Option<CrashPoint>, boundary: CrashPoint) -> Result<(), StoreError> {
    if actual == Some(boundary) {
        Err(StoreError::InjectedCrash(boundary))
    } else {
        Ok(())
    }
}

fn key_hash(key: &IdempotencyKey) -> String {
    hex::encode(Sha256::digest(key.as_str().as_bytes()))
}

fn latest_pointer(directory: &Path) -> Result<Option<PointerRecord>, StoreError> {
    let Some(path) = sorted_files(directory)?.pop() else {
        return Ok(None);
    };
    read_json(&path).map(Some)
}

fn next_pointer_sequence(directory: &Path) -> Result<u64, StoreError> {
    Ok(latest_pointer(directory)?.map_or(1, |pointer| pointer.sequence + 1))
}

fn next_file_sequence(directory: &Path) -> Result<u64, StoreError> {
    let Some(path) = sorted_files(directory)?.pop() else {
        return Ok(1);
    };
    let stem = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or(StoreError::InvalidSequence)?;
    let sequence = stem
        .parse::<u64>()
        .map_err(|_| StoreError::InvalidSequence)?;
    Ok(sequence + 1)
}

fn sorted_files(directory: &Path) -> Result<Vec<PathBuf>, StoreError> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut files = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(std::fs::FileType::is_file)
                .map(|_| entry.path())
        })
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn install_immutable(source: &Path, destination: &Path) -> Result<(), StoreError> {
    if destination.exists() {
        if !source.exists() {
            return Ok(());
        }
        let source_bytes = fs::read(source)?;
        let destination_bytes = fs::read(destination)?;
        if source_bytes == destination_bytes {
            return Ok(());
        }
        return Err(StoreError::ImmutableCollision(destination.to_path_buf()));
    }
    fs::rename(source, destination)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use jingwei_jir::{JirGraph, RevisionSnapshot, RevisionStatus};
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn second_writer_cannot_take_workspace_lease() {
        let directory = tempdir().unwrap();
        let store =
            WorkspaceStore::create(directory.path(), WorkspaceId::new("ws-lock").unwrap()).unwrap();
        let first = store.acquire_writer("first").unwrap();
        assert!(matches!(
            store.acquire_writer("second"),
            Err(StoreError::WriterAlreadyHeld)
        ));
        drop(first);
        store.acquire_writer("third").unwrap();
    }

    #[test]
    fn artifact_round_trip_verifies_content_hash() {
        let directory = tempdir().unwrap();
        let store =
            WorkspaceStore::create(directory.path(), WorkspaceId::new("ws-cas").unwrap()).unwrap();
        let _lease = store.acquire_writer("test").unwrap();
        let record = store.put_artifact(b"evidence", "text/plain").unwrap();
        let (loaded, bytes) = store.artifact(&record.id).unwrap();
        assert_eq!(bytes, b"evidence");
        assert_eq!(loaded.sha256, record.sha256);
    }

    #[test]
    fn every_commit_boundary_recovers_to_one_clean_pointer() {
        for crash in [
            CrashPoint::AfterPrepare,
            CrashPoint::AfterRevision,
            CrashPoint::AfterJournal,
            CrashPoint::AfterPointer,
        ] {
            let directory = tempdir().unwrap();
            let workspace = WorkspaceId::new(format!("ws-{crash:?}")).unwrap();
            let store = WorkspaceStore::create(directory.path(), workspace).unwrap();
            let lease = store.acquire_writer("before-crash").unwrap();
            let project = ProjectId::new("project-recovery").unwrap();
            store
                .create_project(&ProjectMetadata {
                    project_id: project.clone(),
                    name: "recovery".into(),
                    created_at: Utc::now(),
                })
                .unwrap();
            let initial = snapshot(&project, Vec::new(), "initial");
            let initial_id = initial.metadata.id.clone();
            store
                .commit(commit_request(
                    initial,
                    None,
                    1,
                    "initial",
                    PointerTarget::ProjectCurrent,
                ))
                .unwrap();
            let next = snapshot(&project, vec![initial_id.clone()], "next");
            let next_id = next.metadata.id.clone();
            let result = store.commit_with_crash(
                commit_request(
                    next,
                    Some(initial_id.clone()),
                    2,
                    &format!("next-{crash:?}"),
                    PointerTarget::ProjectCurrent,
                ),
                Some(crash),
            );
            assert!(matches!(result, Err(StoreError::InjectedCrash(point)) if point == crash));
            drop(lease);

            let reopened = WorkspaceStore::open(directory.path()).unwrap();
            let _recovery_lease = reopened.acquire_writer("after-crash").unwrap();
            let report = reopened.recover().unwrap();
            assert_eq!(report.recovered_transactions, 1, "crash point {crash:?}");
            assert_eq!(
                reopened.current_revision(&project).unwrap(),
                Some(next_id.clone())
            );
            reopened.revision(&project, &next_id).unwrap();
        }
    }

    fn snapshot(project: &ProjectId, parents: Vec<RevisionId>, marker: &str) -> RevisionSnapshot {
        let mut graph = JirGraph::default();
        let entity = jingwei_jir::JirEntity {
            id: jingwei_jir::EntityId::new(format!("entity-{marker}")).unwrap(),
            kind: jingwei_jir::EntityKind::new("core.test").unwrap(),
            schema_version: jingwei_jir::JIR_SCHEMA_VERSION.into(),
            properties: jingwei_jir::CanonicalValue::new(json!({"marker": marker})).unwrap(),
            provenance: jingwei_jir::Provenance::default(),
            edit_control: jingwei_jir::EditControl::default(),
        };
        graph.insert_entity(entity).unwrap();
        RevisionSnapshot::build(
            project.clone(),
            parents,
            graph,
            "test",
            jingwei_jir::canonical_sha256(&marker).unwrap(),
            RevisionStatus::Candidate,
        )
        .unwrap()
    }

    fn commit_request(
        snapshot: RevisionSnapshot,
        base: Option<RevisionId>,
        sequence: u64,
        key: &str,
        pointer_target: PointerTarget,
    ) -> CommitRequest {
        let project = snapshot.metadata.project_id.clone();
        let result = snapshot.metadata.id.clone();
        let idempotency_key = IdempotencyKey::new(format!("idem-{key}")).unwrap();
        let input_hash = jingwei_jir::canonical_sha256(&key).unwrap();
        CommitRequest {
            snapshot,
            journal: OperationJournalRecord {
                execution_id: ExecutionId::new(format!("execution-{key}")).unwrap(),
                request_id: RequestId::new(format!("request-{key}")).unwrap(),
                project_id: project.clone(),
                idempotency_key: idempotency_key.clone(),
                operation: "test.commit".into(),
                base_revision: base.clone(),
                result_revision: result.clone(),
                normalized_input_hash: input_hash.clone(),
                actor: "test".into(),
                created_at: Utc::now(),
                operations: json!({"key": key}),
                artifact_refs: Vec::new(),
                evidence_refs: Vec::new(),
            },
            idempotency: IdempotencyRecord {
                key: idempotency_key,
                project_id: project.clone(),
                base_revision: base.clone(),
                normalized_input_hash: input_hash,
                result_revision: result.clone(),
                response: json!({"revision": result}),
            },
            event: StoredEvent {
                aggregate_id: project.to_string(),
                sequence,
                timestamp: Utc::now(),
                event_type: "revision.committed".into(),
                data: json!({}),
            },
            pointer_target,
            expected_pointer: base,
        }
    }
}

fn write_new_json(path: &Path, value: &impl Serialize) -> Result<(), StoreError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_new_bytes(path, &bytes)
}

fn write_new_bytes(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, StoreError> {
    let file = File::open(path)?;
    serde_json::from_reader(file).map_err(Into::into)
}

/// Storage, integrity, or recovery failure.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("store I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("store JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("revision verification failed: {0}")]
    Revision(#[from] jingwei_jir::RevisionVerificationError),
    #[error("workspace identity does not match existing metadata")]
    WorkspaceIdentityMismatch,
    #[error("workspace writer lease is already held")]
    WriterAlreadyHeld,
    #[error("workspace is read-only: {0}")]
    ReadOnly(String),
    #[error("project name is empty, overlong, or contains control characters")]
    InvalidProjectName,
    #[error("stale base revision: expected {expected:?}, actual {actual:?}")]
    StaleBase {
        expected: Option<RevisionId>,
        actual: Option<RevisionId>,
    },
    #[error("commit objects do not refer to the same project, revision, key, and input")]
    CommitInvariant,
    #[error("immutable store object collision at {0}")]
    ImmutableCollision(PathBuf),
    #[error("append-only pointer sequence collision")]
    PointerConflict,
    #[error("injected crash at {0:?}")]
    InjectedCrash(CrashPoint),
    #[error("artifact exceeds the {MAX_ARTIFACT_BYTES} byte limit: {0}")]
    ArtifactTooLarge(usize),
    #[error("artifact ID is malformed")]
    InvalidArtifactId,
    #[error("artifact content hash does not match metadata")]
    ArtifactHashMismatch,
    #[error("draft session does not exist: {0}")]
    DraftNotFound(SessionId),
    #[error("append-only sequence filename is malformed")]
    InvalidSequence,
}
