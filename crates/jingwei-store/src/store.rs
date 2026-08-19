//! Workspace store implementation and recovery protocol.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use jingwei_jir::{
    ApprovalAttestation, ArtifactId, AssessmentDependencyState, AssessmentId, ChangeProposal,
    CommitId, ExecutionId, IdempotencyKey, InvocationCorrelation, MaturityAssessment, ProjectId,
    ProposalAction, ProposalDecisionRecord, ProposalId, ReleaseId, ReleaseRecord, ReleaseRequest,
    ReleaseRequestId, RequestId, RevisionCommitRecord, RevisionId, RevisionSnapshot, RevokeRecord,
    SessionId, TaskId, WorkspaceId, assess_validity,
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
    pub commit_id: CommitId,
    pub correlation: InvocationCorrelation,
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

/// Immutable terminal JDP execution ledger used after a response is lost.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRecord {
    pub execution_id: ExecutionId,
    pub project_id: ProjectId,
    pub idempotency_key: IdempotencyKey,
    pub operation: String,
    pub status: String,
    pub response_hash: String,
    pub response: Value,
    pub recorded_at: DateTime<Utc>,
}

/// Durable task projection. Active tasks are reconciled to a fail-closed state after restart.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DurableTaskRecord {
    pub task_id: TaskId,
    pub project_id: ProjectId,
    pub execution_id: ExecutionId,
    pub operation: String,
    pub state: String,
    pub cancellable: bool,
    pub external_side_effects: bool,
    pub terminal: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quarantine_reason: Option<String>,
    #[serde(default)]
    pub repair_actions: Vec<String>,
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
    ProjectHead,
    Draft(SessionId),
    None,
}

/// Validated inputs to one atomic store transaction.
pub struct CommitRequest {
    pub snapshot: RevisionSnapshot,
    pub commit_record: RevisionCommitRecord,
    pub proposal: Option<ChangeProposal>,
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

/// Result of an explicit, backed-up workspace schema migration.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreMigrationReport {
    pub source_version: u32,
    pub target_version: u32,
    pub backup_path: PathBuf,
    pub migrated_revisions: usize,
    pub migrated_commits: usize,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proposal_decision: Option<ProposalDecisionRecord>,
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
    commit_file: String,
    proposal_file: Option<String>,
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
        let read_only_reason = match metadata.schema_version.cmp(&STORE_SCHEMA_VERSION) {
            std::cmp::Ordering::Greater => Some(format!(
                "workspace schema {} is newer than supported {}",
                metadata.schema_version, STORE_SCHEMA_VERSION
            )),
            std::cmp::Ordering::Less => Some(format!(
                "workspace schema {} requires explicit migration to {}",
                metadata.schema_version, STORE_SCHEMA_VERSION
            )),
            std::cmp::Ordering::Equal => None,
        };
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

    /// Migrates a version-1 store to the current schema after creating a complete control backup.
    pub fn migrate(root: impl AsRef<Path>) -> Result<StoreMigrationReport, StoreError> {
        let root = absolute_normalized(root.as_ref())?;
        let layout = Layout::new(root);
        let metadata_path = layout.workspace_metadata();
        let mut metadata: WorkspaceMetadata = read_json(&metadata_path)?;
        if metadata.schema_version > STORE_SCHEMA_VERSION {
            return Err(StoreError::DowngradeRefused {
                found: metadata.schema_version,
                supported: STORE_SCHEMA_VERSION,
            });
        }
        if metadata.schema_version == STORE_SCHEMA_VERSION {
            return Err(StoreError::MigrationNotRequired(STORE_SCHEMA_VERSION));
        }
        if metadata.schema_version != 1 {
            return Err(StoreError::UnsupportedMigration {
                from: metadata.schema_version,
                to: STORE_SCHEMA_VERSION,
            });
        }

        fs::create_dir_all(layout.locks())?;
        let owner_path = layout.locks().join("writer.lock");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&owner_path)?;
        let _migration_lease = WriterLease::acquire(
            lock_file,
            owner_path,
            &WriterOwner {
                instance_id: format!("store-migration-{}", Uuid::new_v4()),
                process_id: std::process::id(),
                acquired_at: Utc::now(),
            },
        )?;
        if layout.transactions().is_dir()
            && fs::read_dir(layout.transactions())?
                .next()
                .transpose()?
                .is_some()
        {
            return Err(StoreError::MigrationPendingTransactions);
        }

        let source_version = metadata.schema_version;
        let backup_root = layout
            .control()
            .join("migration-backups")
            .join(format!("store-v{source_version}-{}", Uuid::new_v4()));
        fs::create_dir_all(&backup_root)?;
        copy_tree_excluding(&layout.control(), &backup_root, "migration-backups")?;

        let mut migrated_revisions = 0;
        let mut migrated_commits = 0;
        for project_entry in fs::read_dir(layout.projects())? {
            let project_entry = project_entry?;
            if !project_entry.file_type()?.is_dir() {
                continue;
            }
            let project_root = project_entry.path();
            let revisions = project_root.join("revisions");
            for revision_path in sorted_files(&revisions)? {
                let mut value: Value = read_json(&revision_path)?;
                let revision_metadata = value
                    .get_mut("metadata")
                    .and_then(Value::as_object_mut)
                    .ok_or(StoreError::LegacyMigrationInvariant)?;
                for legacy_field in [
                    "createdAt",
                    "createdBy",
                    "status",
                    "artifactRefs",
                    "evidenceRefs",
                ] {
                    revision_metadata.remove(legacy_field);
                }
                let snapshot: RevisionSnapshot = serde_json::from_value(value.clone())?;
                snapshot.verify()?;
                replace_json(&revision_path, &value)?;
                migrated_revisions += 1;
            }

            let commits = project_root.join("commits");
            fs::create_dir_all(&commits)?;
            let journal = project_root.join("journal");
            for journal_path in sorted_files(&journal)? {
                let mut value: Value = read_json(&journal_path)?;
                if value.get("commitId").is_some() {
                    continue;
                }
                let legacy_hash = jingwei_jir::canonical_sha256(&value)
                    .map_err(jingwei_jir::RevisionVerificationError::from)?;
                let project_id: ProjectId = value_field(&value, "projectId")?.parse()?;
                let revision_id: RevisionId = value_field(&value, "resultRevision")?.parse()?;
                let request_id: RequestId = value_field(&value, "requestId")?.parse()?;
                let execution_id = value_field(&value, "executionId")?.to_owned();
                let actor = value_field(&value, "actor")?.to_owned();
                let issued_at: DateTime<Utc> = serde_json::from_value(
                    value
                        .get("createdAt")
                        .cloned()
                        .ok_or(StoreError::LegacyMigrationInvariant)?,
                )?;
                let commit_id = CommitId::new(format!("commit-migration-{legacy_hash}"))?;
                let correlation = InvocationCorrelation {
                    invocation_id: jingwei_jir::InvocationId::new(format!(
                        "invocation-migration-{legacy_hash}"
                    ))?,
                    source_kind: jingwei_jir::InvocationSourceKind::SystemMigration,
                    source_id: execution_id,
                    request_id,
                    daemon_instance_id: None,
                    daemon_epoch: None,
                };
                let commit_record = RevisionCommitRecord {
                    id: commit_id.clone(),
                    project_id,
                    revision_id,
                    actor: jingwei_jir::AuthenticatedActorRef {
                        subject: actor,
                        issuer: "legacy-store-v1".into(),
                        authentication_method: "legacy-unverified".into(),
                        role_claims: std::collections::BTreeSet::new(),
                    },
                    issued_at,
                    correlation: correlation.clone(),
                };
                replace_json(&commits.join(format!("{commit_id}.json")), &commit_record)?;
                let journal_object = value
                    .as_object_mut()
                    .ok_or(StoreError::LegacyMigrationInvariant)?;
                journal_object.remove("actor");
                journal_object.remove("createdAt");
                journal_object.insert("commitId".into(), serde_json::to_value(commit_id)?);
                journal_object.insert("correlation".into(), serde_json::to_value(correlation)?);
                replace_json(&journal_path, &value)?;
                migrated_commits += 1;
            }
        }

        metadata.schema_version = STORE_SCHEMA_VERSION;
        replace_json(&metadata_path, &metadata)?;
        let migrated = Self::open(layout.root())?;
        if migrated.read_only_reason().is_some() {
            return Err(StoreError::LegacyMigrationInvariant);
        }
        Ok(StoreMigrationReport {
            source_version,
            target_version: STORE_SCHEMA_VERSION,
            backup_path: backup_root,
            migrated_revisions,
            migrated_commits,
        })
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

    /// Allocates the next durable daemon epoch while the caller holds the writer lease.
    pub fn allocate_daemon_epoch(&self) -> Result<u64, StoreError> {
        self.ensure_writable()?;
        let path = self.layout.daemon_epoch();
        let current = if path.is_file() {
            read_json::<u64>(&path)?
        } else {
            0
        };
        let next = current
            .checked_add(1)
            .ok_or(StoreError::DaemonEpochExhausted)?;
        replace_json(&path, &next)?;
        Ok(next)
    }

    /// Creates immutable project metadata and its subdirectories.
    pub fn create_project(&self, metadata: &ProjectMetadata) -> Result<(), StoreError> {
        self.ensure_writable()?;
        validate_project_name(&metadata.name)?;
        let root = self.layout.project(&metadata.project_id);
        fs::create_dir_all(&root)?;
        for path in [
            self.layout.revisions(&metadata.project_id),
            self.layout.commits(&metadata.project_id),
            self.layout.proposals(&metadata.project_id),
            self.layout.assessments(&metadata.project_id),
            self.layout.attestations(&metadata.project_id),
            self.layout.release_requests(&metadata.project_id),
            self.layout.releases(&metadata.project_id),
            self.layout.revocations(&metadata.project_id),
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

    /// Loads the journal written atomically with a committed revision.
    pub fn operation_journal(
        &self,
        project: &ProjectId,
        execution: &ExecutionId,
    ) -> Result<Option<OperationJournalRecord>, StoreError> {
        let path = self
            .layout
            .journal(project)
            .join(format!("{execution}.json"));
        if path.is_file() {
            read_json(&path).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Records one immutable terminal response before it is returned to a client.
    pub fn record_execution(&self, record: &ExecutionRecord) -> Result<(), StoreError> {
        self.ensure_writable()?;
        fs::create_dir_all(self.layout.executions())?;
        let path = self.layout.execution(&record.execution_id);
        if path.is_file() {
            let existing: ExecutionRecord = read_json(&path)?;
            if existing.response_hash == record.response_hash
                && existing.idempotency_key == record.idempotency_key
                && existing.project_id == record.project_id
                && existing.operation == record.operation
            {
                return Ok(());
            }
            return Err(StoreError::ImmutableCollision(path));
        }
        write_new_json(&path, record)
    }

    /// Reads one terminal execution response by its client-visible execution ID.
    pub fn execution(
        &self,
        execution: &ExecutionId,
    ) -> Result<Option<ExecutionRecord>, StoreError> {
        let path = self.layout.execution(execution);
        if path.is_file() {
            read_json(&path).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Creates or atomically replaces the latest durable task projection.
    pub fn put_task(&self, record: &DurableTaskRecord) -> Result<(), StoreError> {
        self.ensure_writable()?;
        fs::create_dir_all(self.layout.tasks())?;
        replace_json(&self.layout.task(&record.task_id), record)
    }

    /// Reads a durable task projection.
    pub fn task(&self, task: &TaskId) -> Result<Option<DurableTaskRecord>, StoreError> {
        let path = self.layout.task(task);
        if path.is_file() {
            read_json(&path).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Converts tasks interrupted by daemon restart into an explicit safety quarantine.
    pub fn reconcile_incomplete_tasks(&self) -> Result<usize, StoreError> {
        self.ensure_writable()?;
        fs::create_dir_all(self.layout.tasks())?;
        let mut reconciled = 0;
        for path in sorted_files(&self.layout.tasks())? {
            let mut task: DurableTaskRecord = read_json(&path)?;
            if task.terminal {
                continue;
            }
            task.state = "needs-reconciliation".into();
            task.terminal = true;
            task.updated_at = Utc::now();
            task.quarantine_reason =
                Some("daemon restarted before the task produced a confirmed terminal state".into());
            task.repair_actions = vec![
                "inspect isolated process state and output hashes".into(),
                "retry only after an authorized operator resolves the quarantine".into(),
            ];
            replace_json(&path, &task)?;
            reconciled += 1;
        }
        Ok(reconciled)
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
        if let Some(proposal) = &request.proposal {
            let current_head = self.current_revision(project)?;
            if current_head != Some(proposal.base_project_head.clone()) {
                return Err(StoreError::StaleBase {
                    expected: Some(proposal.base_project_head.clone()),
                    actual: current_head,
                });
            }
        }
        let current = self.pointer_revision(project, &request.pointer_target)?;
        if current != request.expected_pointer {
            return Err(StoreError::StaleBase {
                expected: request.expected_pointer,
                actual: current,
            });
        }
        let pointer_sequence = match request.pointer_target {
            PointerTarget::ProjectHead => Some(next_pointer_sequence(
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
        let commit_file = format!("{}.json", request.commit_record.id);
        let proposal_file = request
            .proposal
            .as_ref()
            .map(|proposal| format!("{}.json", hex::encode(proposal.id.as_str().as_bytes())));
        let idempotency_file = format!("{}.json", key_hash(&request.idempotency.key));
        let event_file = format!("{:020}.json", request.event.sequence);
        write_new_json(&transaction.join("revision.json"), &request.snapshot)?;
        write_new_json(&transaction.join("commit.json"), &request.commit_record)?;
        if let Some(proposal) = &request.proposal {
            write_new_json(&transaction.join("proposal.json"), proposal)?;
        }
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
            commit_file,
            proposal_file,
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

    /// Reads one immutable change proposal.
    pub fn proposal(
        &self,
        project: &ProjectId,
        proposal: &ProposalId,
    ) -> Result<ChangeProposal, StoreError> {
        read_json(&self.layout.proposal(project, proposal))
    }

    /// Returns the effective append-only proposal decision, including accepted pointer records.
    pub fn proposal_decision(
        &self,
        project: &ProjectId,
        proposal: &ProposalId,
    ) -> Result<Option<ProposalDecisionRecord>, StoreError> {
        for path in sorted_files(&self.layout.current_pointers(project))? {
            let pointer: PointerRecord = read_json(&path)?;
            if pointer
                .proposal_decision
                .as_ref()
                .is_some_and(|decision| &decision.proposal_id == proposal)
            {
                return Ok(pointer.proposal_decision);
            }
        }
        let mut decisions = sorted_files(&self.layout.proposal_decisions(project, proposal))?;
        decisions
            .pop()
            .map_or(Ok(None), |path| read_json(&path).map(Some))
    }

    /// Stores an immutable, canonical maturity assessment for an existing revision.
    pub fn put_assessment(&self, assessment: &MaturityAssessment) -> Result<(), StoreError> {
        self.ensure_writable()?;
        self.revision(&assessment.project_id, &assessment.revision_id)?;
        let validity = assess_validity(
            assessment,
            &AssessmentDependencyState {
                revision_id: assessment.revision_id.clone(),
                locks: assessment.locks.clone(),
            },
        );
        if !validity.current {
            return Err(StoreError::LifecycleInvariant(
                validity.stale_reasons.join("; "),
            ));
        }
        fs::create_dir_all(self.layout.assessments(&assessment.project_id))?;
        write_new_json(
            &self
                .layout
                .assessment(&assessment.project_id, &assessment.id),
            assessment,
        )
    }

    /// Reads one immutable maturity assessment.
    pub fn assessment(
        &self,
        project: &ProjectId,
        assessment: &AssessmentId,
    ) -> Result<MaturityAssessment, StoreError> {
        read_json(&self.layout.assessment(project, assessment))
    }

    /// Stores one immutable authenticated role attestation.
    pub fn put_attestation(
        &self,
        project: &ProjectId,
        attestation: &ApprovalAttestation,
    ) -> Result<(), StoreError> {
        self.ensure_writable()?;
        let assessment = self.assessment(project, &attestation.assessment_id)?;
        if attestation.assessment_id != assessment.id
            || !attestation.actor.role_claims.contains(&attestation.role)
        {
            return Err(StoreError::LifecycleInvariant(
                "attestation assessment binding or role claim is invalid".to_owned(),
            ));
        }
        fs::create_dir_all(self.layout.attestations(project))?;
        write_new_json(
            &self.layout.attestation(project, &attestation.id),
            attestation,
        )
    }

    /// Reads one immutable authenticated role attestation.
    pub fn attestation(
        &self,
        project: &ProjectId,
        attestation: &jingwei_jir::AttestationId,
    ) -> Result<ApprovalAttestation, StoreError> {
        read_json(&self.layout.attestation(project, attestation))
    }

    /// Stores an unsigned release request prepared by the lifecycle engine.
    pub fn put_release_request(&self, request: &ReleaseRequest) -> Result<(), StoreError> {
        self.ensure_writable()?;
        let assessment = self.assessment(&request.project_id, &request.assessment_id)?;
        if request.revision_id != assessment.revision_id {
            return Err(StoreError::LifecycleInvariant(
                "release request revision does not match assessment".to_owned(),
            ));
        }
        fs::create_dir_all(self.layout.release_requests(&request.project_id))?;
        write_new_json(
            &self
                .layout
                .release_request(&request.project_id, &request.id),
            request,
        )
    }

    /// Reads one immutable release request.
    pub fn release_request(
        &self,
        project: &ProjectId,
        request: &ReleaseRequestId,
    ) -> Result<ReleaseRequest, StoreError> {
        read_json(&self.layout.release_request(project, request))
    }

    /// Stores an externally signed release record without treating it as verified.
    pub fn put_release_record(&self, record: &ReleaseRecord) -> Result<(), StoreError> {
        self.ensure_writable()?;
        let stored = self.release_request(&record.request.project_id, &record.request.id)?;
        if stored != record.request
            || record.signature.is_empty()
            || record.signer_key_id.is_empty()
        {
            return Err(StoreError::LifecycleInvariant(
                "signed release does not match the stored request".to_owned(),
            ));
        }
        fs::create_dir_all(self.layout.releases(&record.request.project_id))?;
        write_new_json(
            &self.layout.release(&record.request.project_id, &record.id),
            record,
        )
    }

    /// Reads one signed release record. Call the offline verifier before trusting it.
    pub fn release_record(
        &self,
        project: &ProjectId,
        release: &ReleaseId,
    ) -> Result<ReleaseRecord, StoreError> {
        read_json(&self.layout.release(project, release))
    }

    /// Appends an externally signed revocation record.
    pub fn put_revocation(
        &self,
        project: &ProjectId,
        record: &RevokeRecord,
    ) -> Result<(), StoreError> {
        self.ensure_writable()?;
        self.release_record(project, &record.release_id)?;
        if record.signature.is_empty() || record.signer_key_id.is_empty() {
            return Err(StoreError::LifecycleInvariant(
                "revocation must carry an external signer and signature".to_owned(),
            ));
        }
        fs::create_dir_all(self.layout.revocations(project))?;
        write_new_json(&self.layout.revocation(project, &record.id), record)
    }

    /// Returns all immutable revocations for one release.
    pub fn revocations(
        &self,
        project: &ProjectId,
        release: &ReleaseId,
    ) -> Result<Vec<RevokeRecord>, StoreError> {
        let mut records = Vec::new();
        for path in sorted_files(&self.layout.revocations(project))? {
            let record: RevokeRecord = read_json(&path)?;
            if &record.release_id == release {
                records.push(record);
            }
        }
        records.sort_by(|left, right| {
            left.issued_at
                .cmp(&right.issued_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(records)
    }

    /// Appends reject/withdraw, or atomically couples accept to a project-head CAS pointer record.
    pub fn decide_proposal(
        &self,
        project: &ProjectId,
        decision: &ProposalDecisionRecord,
    ) -> Result<(), StoreError> {
        self.ensure_writable()?;
        let proposal = self.proposal(project, &decision.proposal_id)?;
        if proposal.project_id != *project {
            return Err(StoreError::CommitInvariant);
        }
        if self.proposal_decision(project, &proposal.id)?.is_some() {
            return Err(StoreError::ProposalAlreadyDecided(proposal.id));
        }
        match decision.action {
            ProposalAction::Accept => {
                let current = self.current_revision(project)?;
                if current != Some(proposal.base_project_head.clone()) {
                    return Err(StoreError::StaleBase {
                        expected: Some(proposal.base_project_head),
                        actual: current,
                    });
                }
                self.revision(project, &proposal.proposed_revision)?;
                let sequence = next_pointer_sequence(&self.layout.current_pointers(project))?;
                let pointer = PointerRecord {
                    sequence,
                    revision_id: proposal.proposed_revision,
                    transaction_id: format!("proposal-{}", decision.id),
                    created_at: Utc::now(),
                    proposal_decision: Some(decision.clone()),
                };
                write_new_json(
                    &self
                        .layout
                        .current_pointers(project)
                        .join(format!("{sequence:020}.json")),
                    &pointer,
                )
            }
            ProposalAction::Reject | ProposalAction::Withdraw => {
                let directory = self.layout.proposal_decisions(project, &proposal.id);
                fs::create_dir_all(&directory)?;
                write_new_json(&directory.join(format!("{}.json", decision.id)), decision)
            }
        }
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
            PointerTarget::ProjectHead => self.current_revision(project),
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
            &transaction.join("commit.json"),
            &self
                .layout
                .commits(&manifest.project_id)
                .join(&manifest.commit_file),
        )?;
        if let Some(proposal_file) = &manifest.proposal_file {
            install_immutable(
                &transaction.join("proposal.json"),
                &self
                    .layout
                    .proposals(&manifest.project_id)
                    .join(proposal_file),
            )?;
        }
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
            PointerTarget::ProjectHead => self.layout.current_pointers(&manifest.project_id),
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
            proposal_decision: None,
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
        layout.executions(),
        layout.tasks(),
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
        || request.snapshot.metadata.id != request.commit_record.revision_id
        || request.snapshot.metadata.project_id != request.journal.project_id
        || request.snapshot.metadata.project_id != request.idempotency.project_id
        || request.snapshot.metadata.project_id != request.commit_record.project_id
        || request.journal.commit_id != request.commit_record.id
        || request.journal.correlation != request.commit_record.correlation
        || request.journal.idempotency_key != request.idempotency.key
        || request.journal.normalized_input_hash != request.idempotency.normalized_input_hash
    {
        return Err(StoreError::CommitInvariant);
    }
    if let Some(proposal) = &request.proposal
        && (proposal.project_id != request.snapshot.metadata.project_id
            || proposal.proposed_revision != request.snapshot.metadata.id
            || proposal.created_by_commit != request.commit_record.id)
    {
        return Err(StoreError::CommitInvariant);
    }
    let journal_hash = jingwei_jir::canonical_sha256(&request.journal.operations)
        .map_err(jingwei_jir::RevisionVerificationError::from)?;
    if request.snapshot.metadata.operation_journal_hash != journal_hash {
        return Err(StoreError::JournalLinkageMismatch);
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
    use jingwei_jir::{
        AuthenticatedActorRef, ChangeProposal, CommitId, DecisionId, DependencyLocks,
        GateObservation, GateStatus, InvocationCorrelation, InvocationId, InvocationSourceKind,
        JirGraph, ProposalAction, ProposalDecisionRecord, ProposalId, RevisionCommitRecord,
        RevisionSnapshot, build_maturity_assessment,
    };
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
    fn lifecycle_assessments_are_immutable_and_canonical() {
        let directory = tempdir().unwrap();
        let store =
            WorkspaceStore::create(directory.path(), WorkspaceId::new("ws-lifecycle").unwrap())
                .unwrap();
        let _lease = store.acquire_writer("lifecycle-test").unwrap();
        let project = ProjectId::new("project-lifecycle").unwrap();
        store
            .create_project(&ProjectMetadata {
                project_id: project.clone(),
                name: "lifecycle".into(),
                created_at: Utc::now(),
            })
            .unwrap();
        let initial = snapshot(&project, Vec::new(), "lifecycle-initial");
        let revision = initial.metadata.id.clone();
        store
            .commit(commit_request(
                initial,
                None,
                1,
                "lifecycle-initial",
                PointerTarget::ProjectHead,
            ))
            .unwrap();
        let locks = DependencyLocks {
            intent_revision: "intent:1".into(),
            toolchain_lock: "toolchain:1".into(),
            rule_set_lock: "rules:1".into(),
            manufacturer_profile_lock: "profile:1".into(),
            evidence_set_hash: "evidence:1".into(),
            waiver_set_hash: "waiver:1".into(),
        };
        let assessment = build_maturity_assessment(
            project.clone(),
            revision,
            locks,
            vec![GateObservation {
                gate: "G0".into(),
                status: GateStatus::Passed,
                finding_refs: Vec::new(),
                provider: "fixture".into(),
            }],
            Utc::now(),
        )
        .unwrap();
        store.put_assessment(&assessment).unwrap();
        assert_eq!(
            store.assessment(&project, &assessment.id).unwrap(),
            assessment
        );
        assert!(matches!(
            store.put_assessment(&assessment),
            Err(StoreError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists
        ));
        let mut tampered = assessment;
        tampered.locks.rule_set_lock = "rules:2".into();
        assert!(matches!(
            store.put_assessment(&tampered),
            Err(StoreError::LifecycleInvariant(_))
        ));
    }

    #[test]
    fn version_one_store_requires_explicit_backed_up_migration() {
        let directory = tempdir().unwrap();
        let store = WorkspaceStore::create(
            directory.path(),
            WorkspaceId::new("ws-migration-v1").unwrap(),
        )
        .unwrap();
        let lease = store.acquire_writer("migration-fixture").unwrap();
        let project = ProjectId::new("project-migration-v1").unwrap();
        store
            .create_project(&ProjectMetadata {
                project_id: project.clone(),
                name: "legacy".into(),
                created_at: Utc::now(),
            })
            .unwrap();
        let initial = snapshot(&project, Vec::new(), "legacy-initial");
        let revision_id = initial.metadata.id.clone();
        store
            .commit(commit_request(
                initial,
                None,
                1,
                "legacy-initial",
                PointerTarget::ProjectHead,
            ))
            .unwrap();
        drop(lease);

        let revision_path = store.layout.revision(&project, &revision_id);
        let mut revision_value: Value = read_json(&revision_path).unwrap();
        let metadata = revision_value["metadata"].as_object_mut().unwrap();
        metadata.insert("createdAt".into(), json!(Utc::now()));
        metadata.insert("createdBy".into(), json!("legacy-user"));
        metadata.insert("status".into(), json!("candidate"));
        metadata.insert("artifactRefs".into(), json!([]));
        metadata.insert("evidenceRefs".into(), json!([]));
        replace_json(&revision_path, &revision_value).unwrap();

        let journal_path = sorted_files(&store.layout.journal(&project))
            .unwrap()
            .pop()
            .unwrap();
        let mut journal_value: Value = read_json(&journal_path).unwrap();
        let journal_object = journal_value.as_object_mut().unwrap();
        journal_object.remove("commitId");
        journal_object.remove("correlation");
        journal_object.insert("actor".into(), json!("legacy-user"));
        journal_object.insert("createdAt".into(), json!(Utc::now()));
        replace_json(&journal_path, &journal_value).unwrap();
        fs::remove_dir_all(store.layout.commits(&project)).unwrap();

        let mut metadata: WorkspaceMetadata =
            read_json(&store.layout.workspace_metadata()).unwrap();
        metadata.schema_version = 1;
        replace_json(&store.layout.workspace_metadata(), &metadata).unwrap();

        let legacy = WorkspaceStore::open(directory.path()).unwrap();
        assert!(
            legacy
                .read_only_reason()
                .unwrap()
                .contains("requires explicit migration")
        );
        assert!(matches!(
            legacy.acquire_writer("must-fail"),
            Err(StoreError::ReadOnly(_))
        ));

        let report = WorkspaceStore::migrate(directory.path()).unwrap();
        assert_eq!(report.source_version, 1);
        assert_eq!(report.target_version, STORE_SCHEMA_VERSION);
        assert_eq!(report.migrated_revisions, 1);
        assert_eq!(report.migrated_commits, 1);
        assert!(report.backup_path.join("workspace.json").is_file());

        let migrated = WorkspaceStore::open(directory.path()).unwrap();
        assert!(migrated.read_only_reason().is_none());
        migrated.revision(&project, &revision_id).unwrap();
        assert_eq!(
            sorted_files(&migrated.layout.commits(&project))
                .unwrap()
                .len(),
            1
        );
        let journal: OperationJournalRecord = read_json(&journal_path).unwrap();
        assert_eq!(
            journal.correlation.source_kind,
            InvocationSourceKind::SystemMigration
        );
    }

    #[test]
    fn newer_store_is_read_only_and_refuses_downgrade() {
        let directory = tempdir().unwrap();
        let store = WorkspaceStore::create(
            directory.path(),
            WorkspaceId::new("ws-newer-store").unwrap(),
        )
        .unwrap();
        let mut metadata = store.metadata().clone();
        metadata.schema_version = STORE_SCHEMA_VERSION + 1;
        replace_json(&store.layout.workspace_metadata(), &metadata).unwrap();

        let newer = WorkspaceStore::open(directory.path()).unwrap();
        assert!(
            newer
                .read_only_reason()
                .unwrap()
                .contains("newer than supported")
        );
        assert!(matches!(
            WorkspaceStore::migrate(directory.path()),
            Err(StoreError::DowngradeRefused { .. })
        ));
    }

    #[test]
    fn migration_refuses_an_active_writer_or_pending_transaction() {
        let directory = tempdir().unwrap();
        let store = WorkspaceStore::create(
            directory.path(),
            WorkspaceId::new("ws-migration-safety").unwrap(),
        )
        .unwrap();
        let lease = store.acquire_writer("active-writer").unwrap();
        let mut metadata = store.metadata().clone();
        metadata.schema_version = 1;
        replace_json(&store.layout.workspace_metadata(), &metadata).unwrap();
        assert!(matches!(
            WorkspaceStore::migrate(directory.path()),
            Err(StoreError::WriterAlreadyHeld)
        ));
        drop(lease);

        fs::create_dir_all(store.layout.transactions().join("pending")).unwrap();
        assert!(matches!(
            WorkspaceStore::migrate(directory.path()),
            Err(StoreError::MigrationPendingTransactions)
        ));
    }

    #[test]
    fn proposals_do_not_move_head_until_one_atomic_accept_decision() {
        let directory = tempdir().unwrap();
        let store = WorkspaceStore::create(
            directory.path(),
            WorkspaceId::new("ws-proposal-cas").unwrap(),
        )
        .unwrap();
        let _lease = store.acquire_writer("proposal-test").unwrap();
        let project = ProjectId::new("project-proposal-cas").unwrap();
        store
            .create_project(&ProjectMetadata {
                project_id: project.clone(),
                name: "proposal-cas".into(),
                created_at: Utc::now(),
            })
            .unwrap();
        let initial = snapshot(&project, Vec::new(), "proposal-initial");
        let initial_id = initial.metadata.id.clone();
        store
            .commit(commit_request(
                initial,
                None,
                1,
                "proposal-initial",
                PointerTarget::ProjectHead,
            ))
            .unwrap();

        let first = snapshot(&project, vec![initial_id.clone()], "proposal-first");
        let first_id = first.metadata.id.clone();
        let first_proposal_id = ProposalId::new("proposal-first").unwrap();
        let mut first_request =
            commit_request(first, None, 2, "proposal-first", PointerTarget::None);
        first_request.proposal = Some(ChangeProposal {
            id: first_proposal_id.clone(),
            project_id: project.clone(),
            base_project_head: initial_id.clone(),
            proposed_revision: first_id.clone(),
            created_by_commit: first_request.commit_record.id.clone(),
            issued_at: Utc::now(),
        });
        store.commit(first_request).unwrap();

        let second = snapshot(&project, vec![initial_id.clone()], "proposal-second");
        let second_id = second.metadata.id.clone();
        let second_proposal_id = ProposalId::new("proposal-second").unwrap();
        let mut second_request =
            commit_request(second, None, 3, "proposal-second", PointerTarget::None);
        second_request.proposal = Some(ChangeProposal {
            id: second_proposal_id.clone(),
            project_id: project.clone(),
            base_project_head: initial_id.clone(),
            proposed_revision: second_id,
            created_by_commit: second_request.commit_record.id.clone(),
            issued_at: Utc::now(),
        });
        store.commit(second_request).unwrap();
        assert_eq!(store.current_revision(&project).unwrap(), Some(initial_id));

        let accept_first = ProposalDecisionRecord {
            id: DecisionId::new("decision-accept-first").unwrap(),
            proposal_id: first_proposal_id.clone(),
            action: ProposalAction::Accept,
            actor: AuthenticatedActorRef {
                subject: "reviewer".into(),
                issuer: "test".into(),
                authentication_method: "test".into(),
                role_claims: ["hardware-reviewer".into()].into_iter().collect(),
            },
            issued_at: Utc::now(),
            reason: Some("reviewed".into()),
        };
        store.decide_proposal(&project, &accept_first).unwrap();
        assert_eq!(store.current_revision(&project).unwrap(), Some(first_id));
        assert_eq!(
            store
                .proposal_decision(&project, &first_proposal_id)
                .unwrap(),
            Some(accept_first)
        );

        let stale_accept = ProposalDecisionRecord {
            id: DecisionId::new("decision-stale-second").unwrap(),
            proposal_id: second_proposal_id,
            action: ProposalAction::Accept,
            actor: AuthenticatedActorRef {
                subject: "reviewer".into(),
                issuer: "test".into(),
                authentication_method: "test".into(),
                role_claims: std::collections::BTreeSet::new(),
            },
            issued_at: Utc::now(),
            reason: None,
        };
        assert!(matches!(
            store.decide_proposal(&project, &stale_accept),
            Err(StoreError::StaleBase { .. })
        ));
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
                    PointerTarget::ProjectHead,
                ))
                .unwrap();
            let next_key = format!("next-{crash:?}");
            let next = snapshot(&project, vec![initial_id.clone()], &next_key);
            let next_id = next.metadata.id.clone();
            let result = store.commit_with_crash(
                commit_request(
                    next,
                    Some(initial_id.clone()),
                    2,
                    &next_key,
                    PointerTarget::ProjectHead,
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
            jingwei_jir::canonical_sha256(&json!({"key": marker})).unwrap(),
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
        let request_id = RequestId::new(format!("request-{key}")).unwrap();
        let commit_id = CommitId::generate();
        let correlation = InvocationCorrelation {
            invocation_id: InvocationId::generate(),
            source_kind: InvocationSourceKind::SystemMigration,
            source_id: "store-test".into(),
            request_id: request_id.clone(),
            daemon_instance_id: Some("daemon-test".into()),
            daemon_epoch: Some(1),
        };
        CommitRequest {
            snapshot,
            commit_record: RevisionCommitRecord {
                id: commit_id.clone(),
                project_id: project.clone(),
                revision_id: result.clone(),
                actor: AuthenticatedActorRef {
                    subject: "store-test".into(),
                    issuer: "test".into(),
                    authentication_method: "test".into(),
                    role_claims: std::collections::BTreeSet::new(),
                },
                issued_at: Utc::now(),
                correlation: correlation.clone(),
            },
            proposal: None,
            journal: OperationJournalRecord {
                execution_id: ExecutionId::new(format!("execution-{key}")).unwrap(),
                request_id,
                project_id: project.clone(),
                idempotency_key: idempotency_key.clone(),
                operation: "test.commit".into(),
                base_revision: base.clone(),
                result_revision: result.clone(),
                normalized_input_hash: input_hash.clone(),
                commit_id,
                correlation,
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

fn replace_json(path: &Path, value: &impl Serialize) -> Result<(), StoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("migration-{}.tmp", Uuid::new_v4()));
    write_new_json(&temporary, value)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}

fn copy_tree_excluding(
    source: &Path,
    destination: &Path,
    excluded_name: &str,
) -> Result<(), StoreError> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_name() == std::ffi::OsStr::new(excluded_name)
            || entry.file_name() == std::ffi::OsStr::new("writer.lock")
        {
            continue;
        }
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree_excluding(&entry.path(), &target, excluded_name)?;
        } else if entry.file_type()?.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn value_field<'a>(value: &'a Value, field: &str) -> Result<&'a str, StoreError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or(StoreError::LegacyMigrationInvariant)
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
    #[error("workspace schema {found} is newer than supported {supported}; downgrade refused")]
    DowngradeRefused { found: u32, supported: u32 },
    #[error("workspace schema {0} is already current; migration is not required")]
    MigrationNotRequired(u32),
    #[error("no supported store migration from schema {from} to {to}")]
    UnsupportedMigration { from: u32, to: u32 },
    #[error("legacy store does not contain the fields required for a safe migration")]
    LegacyMigrationInvariant,
    #[error(
        "workspace has pending transactions; recover them with the matching old version before migration"
    )]
    MigrationPendingTransactions,
    #[error("workspace daemon epoch counter is exhausted")]
    DaemonEpochExhausted,
    #[error("change proposal has already been decided: {0}")]
    ProposalAlreadyDecided(ProposalId),
    #[error(transparent)]
    InvalidId(#[from] jingwei_jir::InvalidId),
    #[error("project name is empty, overlong, or contains control characters")]
    InvalidProjectName,
    #[error("stale base revision: expected {expected:?}, actual {actual:?}")]
    StaleBase {
        expected: Option<RevisionId>,
        actual: Option<RevisionId>,
    },
    #[error("commit objects do not refer to the same project, revision, key, and input")]
    CommitInvariant,
    #[error("lifecycle record invariant failed: {0}")]
    LifecycleInvariant(String),
    #[error("revision operation journal hash does not match the committed journal operations")]
    JournalLinkageMismatch,
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
