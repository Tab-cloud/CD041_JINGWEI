//! Persisted leased draft branches with preview, commit, inverse undo, reapply redo, and publish.

use chrono::{DateTime, Duration, Utc};
use jingwei_jdp::{
    EditCommandGroup, EditCommitOutput, EditPreviewOutput, EditPublishOutput, EditSessionInfo,
    PatchOperation, SemanticEditCommand,
};
use jingwei_jir::{
    CanonicalValue, IdempotencyKey, ProjectId, RequestId, RevisionId, RevisionSnapshot, SessionId,
};
use jingwei_store::PointerTarget;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Kernel, KernelError, RequestContext, apply_patch_to_graph, kernel::CommitMeta};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DraftState {
    session_id: SessionId,
    project_id: ProjectId,
    base_project_revision: RevisionId,
    draft_head_revision: RevisionId,
    lease_expires_at: DateTime<Utc>,
    state: String,
    history: Vec<DraftHistory>,
    cursor: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DraftHistory {
    forward: Vec<PatchOperation>,
    inverse: Vec<PatchOperation>,
}

impl Kernel {
    /// Opens and durably records a draft branch without moving the project pointer.
    #[allow(clippy::too_many_arguments)]
    pub fn open_edit_session(
        &self,
        project_id: &ProjectId,
        base_revision: &RevisionId,
        lease_seconds: u64,
        request_id: RequestId,
        idempotency_key: IdempotencyKey,
        context: &RequestContext,
    ) -> Result<EditSessionInfo, KernelError> {
        require_write(context)?;
        let _claim = self.claim_project_write(project_id, &request_id)?;
        let current = self
            .store
            .current_revision(project_id)?
            .ok_or_else(|| KernelError::CorruptProject("project has no current revision".into()))?;
        if &current != base_revision {
            return Err(KernelError::StaleBase {
                expected: base_revision.clone(),
                actual: current,
            });
        }
        let source = self.store.revision(project_id, base_revision)?;
        let session_id = SessionId::generate();
        let operations = json!({"kind": "draft-open", "sessionId": session_id});
        let snapshot = RevisionSnapshot::build(
            project_id.clone(),
            vec![base_revision.clone()],
            source.graph,
            jingwei_jir::canonical_sha256(&operations)?,
        )?;
        let lease_expires_at = Utc::now()
            + Duration::seconds(i64::try_from(lease_seconds.clamp(30, 86_400)).unwrap_or(86_400));
        let info = EditSessionInfo {
            edit_session_id: session_id.clone(),
            base_project_revision: base_revision.clone(),
            draft_head_revision: snapshot.metadata.id.clone(),
            lease_expires_at,
            state: "active".into(),
        };
        self.commit(
            snapshot,
            CommitMeta {
                request_id,
                idempotency_key,
                operation_name: "edit_session.open",
                base_revision: Some(base_revision.clone()),
                input_hash: jingwei_jir::canonical_sha256(&json!({
                    "baseRevision": base_revision,
                    "leaseSeconds": lease_seconds,
                }))?,
                operations,
                response: serde_json::to_value(&info)?,
                pointer_target: PointerTarget::Draft(session_id.clone()),
                expected_pointer: None,
                proposal: None,
            },
            context,
        )?;
        self.store.append_draft_metadata(
            project_id,
            &session_id,
            &DraftState {
                session_id: session_id.clone(),
                project_id: project_id.clone(),
                base_project_revision: base_revision.clone(),
                draft_head_revision: info.draft_head_revision.clone(),
                lease_expires_at,
                state: "active".into(),
                history: Vec::new(),
                cursor: 0,
            },
        )?;
        Ok(info)
    }

    /// Translates and validates commands, but never creates a revision.
    pub async fn preview_edit(
        &self,
        project_id: &ProjectId,
        group: &EditCommandGroup,
        context: &RequestContext,
    ) -> Result<EditPreviewOutput, KernelError> {
        require_write(context)?;
        let state = self.active_draft(project_id, &group.edit_session_id)?;
        if state.draft_head_revision != group.base_draft_revision {
            return Err(KernelError::StaleBase {
                expected: group.base_draft_revision.clone(),
                actual: state.draft_head_revision,
            });
        }
        let snapshot = self
            .store
            .revision(project_id, &group.base_draft_revision)?;
        let operations = self.translate_commands(&snapshot.graph, &group.commands)?;
        let applied = apply_patch_to_graph(&snapshot.graph, &operations, context)?;
        let validation = self
            .validate_graph(
                group.base_draft_revision.clone(),
                &applied.graph,
                Vec::new(),
                group.expected_postconditions.clone(),
            )
            .await;
        let preview_hash = jingwei_jir::canonical_sha256(&json!({
            "base": group.base_draft_revision,
            "commands": group.commands,
            "operations": operations,
        }))?;
        Ok(EditPreviewOutput {
            preview_hash,
            validation,
            display_delta: Vec::new(),
        })
    }

    /// Commits a previewed command group as one immutable draft revision.
    #[allow(clippy::too_many_arguments)]
    pub async fn commit_edit(
        &self,
        project_id: &ProjectId,
        group: &EditCommandGroup,
        request_id: RequestId,
        idempotency_key: IdempotencyKey,
        context: &RequestContext,
    ) -> Result<EditCommitOutput, KernelError> {
        require_write(context)?;
        let _claim = self.claim_project_write(project_id, &request_id)?;
        let mut state = self.active_draft(project_id, &group.edit_session_id)?;
        if state.draft_head_revision != group.base_draft_revision {
            return Err(KernelError::StaleBase {
                expected: group.base_draft_revision.clone(),
                actual: state.draft_head_revision,
            });
        }
        let source = self
            .store
            .revision(project_id, &group.base_draft_revision)?;
        let operations = self.translate_commands(&source.graph, &group.commands)?;
        let expected_preview = jingwei_jir::canonical_sha256(&json!({
            "base": group.base_draft_revision,
            "commands": group.commands,
            "operations": operations,
        }))?;
        if group.preview_hash.as_deref() != Some(&expected_preview) {
            return Err(KernelError::Precondition(
                "preview hash is missing or does not match commands".into(),
            ));
        }
        let applied = apply_patch_to_graph(&source.graph, &operations, context)?;
        let operation_record = json!({
            "kind": "edit-command",
            "commands": group.commands,
            "forward": operations,
            "inverse": applied.inverse,
            "previewHash": expected_preview,
        });
        let snapshot = RevisionSnapshot::build(
            project_id.clone(),
            vec![group.base_draft_revision.clone()],
            applied.graph,
            jingwei_jir::canonical_sha256(&operation_record)?,
        )?;
        let validation = self
            .validate_graph(
                snapshot.metadata.id.clone(),
                &snapshot.graph,
                Vec::new(),
                group.expected_postconditions.clone(),
            )
            .await;
        if !validation.valid {
            return Err(KernelError::Postcondition(format!(
                "draft graph failed registered validation: {} finding(s)",
                validation.findings.len()
            )));
        }
        let output = EditCommitOutput {
            draft_revision: snapshot.metadata.id.clone(),
            validation,
            diff: source.graph.diff(&snapshot.graph),
        };
        self.commit(
            snapshot,
            CommitMeta {
                request_id,
                idempotency_key,
                operation_name: "edit_command.commit",
                base_revision: Some(group.base_draft_revision.clone()),
                input_hash: jingwei_jir::canonical_sha256(group)?,
                operations: operation_record,
                response: serde_json::to_value(&output)?,
                pointer_target: PointerTarget::Draft(group.edit_session_id.clone()),
                expected_pointer: Some(group.base_draft_revision.clone()),
                proposal: None,
            },
            context,
        )?;
        state.history.truncate(state.cursor);
        state.history.push(DraftHistory {
            forward: operations,
            inverse: applied.inverse,
        });
        state.cursor += 1;
        state.draft_head_revision = output.draft_revision.clone();
        self.store
            .append_draft_metadata(project_id, &group.edit_session_id, &state)?;
        Ok(output)
    }

    /// Creates a new draft revision by applying the prior command group's inverse.
    pub async fn undo_edit(
        &self,
        project_id: &ProjectId,
        session_id: &SessionId,
        request_id: RequestId,
        idempotency_key: IdempotencyKey,
        context: &RequestContext,
    ) -> Result<EditCommitOutput, KernelError> {
        let state = self.active_draft(project_id, session_id)?;
        if state.cursor == 0 {
            return Err(KernelError::InvalidPatch(
                "draft has nothing to undo".into(),
            ));
        }
        let operations = state.history[state.cursor - 1].inverse.clone();
        self.apply_history_step(
            project_id,
            state,
            operations,
            false,
            request_id,
            idempotency_key,
            context,
        )
        .await
    }

    /// Creates a new draft revision by reapplying the next command group.
    pub async fn redo_edit(
        &self,
        project_id: &ProjectId,
        session_id: &SessionId,
        request_id: RequestId,
        idempotency_key: IdempotencyKey,
        context: &RequestContext,
    ) -> Result<EditCommitOutput, KernelError> {
        let state = self.active_draft(project_id, session_id)?;
        if state.cursor >= state.history.len() {
            return Err(KernelError::InvalidPatch(
                "draft has nothing to redo".into(),
            ));
        }
        let operations = state.history[state.cursor].forward.clone();
        self.apply_history_step(
            project_id,
            state,
            operations,
            true,
            request_id,
            idempotency_key,
            context,
        )
        .await
    }

    /// Adds a durable metadata checkpoint and extends the lease without changing JIR.
    pub fn checkpoint_edit(
        &self,
        project_id: &ProjectId,
        session_id: &SessionId,
        context: &RequestContext,
    ) -> Result<EditSessionInfo, KernelError> {
        require_write(context)?;
        let owner = RequestId::generate();
        let _claim = self.claim_project_write(project_id, &owner)?;
        let mut state = self.active_draft(project_id, session_id)?;
        state.lease_expires_at = Utc::now() + Duration::minutes(30);
        self.store
            .append_draft_metadata(project_id, session_id, &state)?;
        Ok(state.info())
    }

    /// Publishes the draft graph only if project current still equals its original base.
    pub fn publish_edit(
        &self,
        project_id: &ProjectId,
        session_id: &SessionId,
        request_id: RequestId,
        idempotency_key: IdempotencyKey,
        context: &RequestContext,
    ) -> Result<EditPublishOutput, KernelError> {
        require_write(context)?;
        let _claim = self.claim_project_write(project_id, &request_id)?;
        let mut state = self.active_draft(project_id, session_id)?;
        let current = self
            .store
            .current_revision(project_id)?
            .ok_or_else(|| KernelError::CorruptProject("project has no current revision".into()))?;
        if current != state.base_project_revision {
            return Err(KernelError::StaleBase {
                expected: state.base_project_revision,
                actual: current,
            });
        }
        let draft = self
            .store
            .revision(project_id, &state.draft_head_revision)?;
        let base = self.store.revision(project_id, &current)?;
        let operations = json!({"kind": "draft-publish", "sessionId": session_id});
        let snapshot = RevisionSnapshot::build(
            project_id.clone(),
            vec![current.clone(), state.draft_head_revision.clone()],
            draft.graph,
            jingwei_jir::canonical_sha256(&operations)?,
        )?;
        let proposal_id = jingwei_jir::ProposalId::generate();
        let output = EditPublishOutput {
            revision: snapshot.metadata.id.clone(),
            proposal_id: proposal_id.clone(),
            diff: base.graph.diff(&snapshot.graph),
        };
        self.commit(
            snapshot,
            CommitMeta {
                request_id,
                idempotency_key,
                operation_name: "edit.publish",
                base_revision: Some(current.clone()),
                input_hash: jingwei_jir::canonical_sha256(&json!({
                    "sessionId": session_id,
                    "draftHead": state.draft_head_revision,
                }))?,
                operations,
                response: serde_json::to_value(&output)?,
                pointer_target: PointerTarget::None,
                expected_pointer: None,
                proposal: Some(crate::kernel::ProposalMeta {
                    id: proposal_id,
                    base_project_head: current,
                }),
            },
            context,
        )?;
        state.state = "published".into();
        self.store
            .append_draft_metadata(project_id, session_id, &state)?;
        Ok(output)
    }

    #[allow(clippy::too_many_arguments)]
    async fn apply_history_step(
        &self,
        project_id: &ProjectId,
        mut state: DraftState,
        operations: Vec<PatchOperation>,
        redo: bool,
        request_id: RequestId,
        idempotency_key: IdempotencyKey,
        context: &RequestContext,
    ) -> Result<EditCommitOutput, KernelError> {
        require_write(context)?;
        let _claim = self.claim_project_write(project_id, &request_id)?;
        let source = self
            .store
            .revision(project_id, &state.draft_head_revision)?;
        let applied = apply_patch_to_graph(&source.graph, &operations, context)?;
        let record = json!({"kind": if redo {"redo"} else {"undo"}, "operations": operations});
        let snapshot = RevisionSnapshot::build(
            project_id.clone(),
            vec![state.draft_head_revision.clone()],
            applied.graph,
            jingwei_jir::canonical_sha256(&record)?,
        )?;
        let validation = self
            .validate_graph(
                snapshot.metadata.id.clone(),
                &snapshot.graph,
                Vec::new(),
                vec!["graph.integrity".into(), "native-erc".into()],
            )
            .await;
        if !validation.valid {
            return Err(KernelError::Postcondition(format!(
                "draft history step failed registered validation: {} finding(s)",
                validation.findings.len()
            )));
        }
        let output = EditCommitOutput {
            draft_revision: snapshot.metadata.id.clone(),
            validation,
            diff: source.graph.diff(&snapshot.graph),
        };
        self.commit(
            snapshot,
            CommitMeta {
                request_id,
                idempotency_key,
                operation_name: if redo { "edit.redo" } else { "edit.undo" },
                base_revision: Some(state.draft_head_revision.clone()),
                input_hash: jingwei_jir::canonical_sha256(&record)?,
                operations: record,
                response: serde_json::to_value(&output)?,
                pointer_target: PointerTarget::Draft(state.session_id.clone()),
                expected_pointer: Some(state.draft_head_revision.clone()),
                proposal: None,
            },
            context,
        )?;
        if redo {
            state.cursor += 1;
        } else {
            state.cursor -= 1;
        }
        state.draft_head_revision = output.draft_revision.clone();
        self.store
            .append_draft_metadata(project_id, &state.session_id, &state)?;
        Ok(output)
    }

    fn active_draft(
        &self,
        project_id: &ProjectId,
        session_id: &SessionId,
    ) -> Result<DraftState, KernelError> {
        let state: DraftState = self.store.draft_metadata(project_id, session_id)?;
        if state.state != "active" {
            return Err(KernelError::InvalidPatch(format!(
                "draft state is {}",
                state.state
            )));
        }
        if state.lease_expires_at <= Utc::now() {
            return Err(KernelError::Precondition("draft lease expired".into()));
        }
        Ok(state)
    }

    fn translate_commands(
        &self,
        graph: &jingwei_jir::JirGraph,
        commands: &[SemanticEditCommand],
    ) -> Result<Vec<PatchOperation>, KernelError> {
        let mut operations = Vec::new();
        for command in commands {
            if command.kind == "core.set_property" {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Input {
                    entity_id: jingwei_jir::EntityId,
                    key: String,
                    value: Value,
                }
                let input: Input = serde_json::from_value(command.payload.clone())?;
                operations.push(PatchOperation::SetProperty {
                    entity_id: input.entity_id,
                    key: input.key,
                    value: CanonicalValue::new(input.value)?,
                });
            } else if command.kind == "core.set_edit_policy" {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Input {
                    entity_ids: Vec<jingwei_jir::EntityId>,
                    policy: jingwei_jir::EditPolicy,
                }
                let input: Input = serde_json::from_value(command.payload.clone())?;
                if input.entity_ids.is_empty() {
                    return Err(KernelError::InvalidPatch(
                        "edit policy command requires at least one entity".into(),
                    ));
                }
                operations.extend(input.entity_ids.into_iter().map(|entity_id| {
                    PatchOperation::SetEditPolicy {
                        entity_id,
                        policy: input.policy,
                    }
                }));
            } else if let Some(handler) = self.registry.edit_handler(&command.kind) {
                operations.extend(
                    handler
                        .translate(graph, command)
                        .map_err(KernelError::InvalidPatch)?,
                );
            } else {
                return Err(KernelError::CapabilityUnavailable(command.kind.clone()));
            }
        }
        Ok(operations)
    }
}

impl DraftState {
    fn info(&self) -> EditSessionInfo {
        EditSessionInfo {
            edit_session_id: self.session_id.clone(),
            base_project_revision: self.base_project_revision.clone(),
            draft_head_revision: self.draft_head_revision.clone(),
            lease_expires_at: self.lease_expires_at,
            state: self.state.clone(),
        }
    }
}

fn require_write(context: &RequestContext) -> Result<(), KernelError> {
    if context.has("design.write") {
        Ok(())
    } else {
        Err(KernelError::PermissionDenied("design.write".into()))
    }
}
