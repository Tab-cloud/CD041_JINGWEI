//! Authoritative transaction service.

use std::collections::BTreeSet;

use chrono::Utc;
use jingwei_jdp::{
    CompileJdlOutput, DiffOutput, ExplainOutput, PatchInput, PatchOutput, PatchProposal,
    ProjectInfo, ProposalDecisionInput, ProposalDecisionOutput, ProtocolDiagnostic, QueryInput,
    QueryOutput, ValidateInput, ValidationFinding, ValidationOutput,
};
use jingwei_jir::{
    AuthenticatedActorRef, CanonicalValue, ChangeProposal, CommitId, DecisionId, EditControl,
    EntityKind, ExecutionId, IdempotencyKey, InvocationCorrelation, InvocationId,
    InvocationSourceKind, JirEntity, JirGraph, ProjectId, ProposalDecisionRecord, ProposalId,
    Provenance, RequestId, RevisionCommitRecord, RevisionId, RevisionSnapshot,
};
use jingwei_store::{
    CommitRequest, IdempotencyRecord, OperationJournalRecord, PointerTarget, ProjectMetadata,
    StoredEvent, WorkspaceStore,
};
use serde_json::{Value, json};

use crate::{
    ClaimGuard, ClaimMode, ExtensionRegistry, ResourceClaim, ResourceClaimManager, ToolContext,
    ToolRequest, ToolResult, ValidationContext, apply_patch_to_graph, evaluate_preconditions,
};

/// Caller identity and daemon-authorized capabilities.
#[derive(Clone, Debug)]
pub struct RequestContext {
    pub actor: AuthenticatedActorRef,
    pub invocation_source: InvocationSourceKind,
    pub invocation_source_id: String,
    pub daemon_instance_id: Option<String>,
    pub daemon_epoch: Option<u64>,
    pub capabilities: BTreeSet<String>,
}

impl RequestContext {
    /// Local CLI/test context with ordinary read and candidate-write authority.
    pub fn local(actor: impl Into<String>) -> Self {
        Self {
            actor: AuthenticatedActorRef {
                subject: actor.into(),
                issuer: "local-os".into(),
                authentication_method: "local-development".into(),
                role_claims: BTreeSet::new(),
            },
            invocation_source: InvocationSourceKind::CliCommand,
            invocation_source_id: "local-cli".into(),
            daemon_instance_id: None,
            daemon_epoch: None,
            capabilities: ["design.read".into(), "design.write".into()]
                .into_iter()
                .collect(),
        }
    }

    /// Binds a daemon-authenticated command invocation to one process epoch.
    #[must_use]
    pub fn for_daemon_command(
        mut self,
        command_invocation_id: impl Into<String>,
        daemon_instance_id: impl Into<String>,
        daemon_epoch: u64,
    ) -> Self {
        self.invocation_source = InvocationSourceKind::UiCommand;
        self.invocation_source_id = command_invocation_id.into();
        self.daemon_instance_id = Some(daemon_instance_id.into());
        self.daemon_epoch = Some(daemon_epoch);
        self
    }

    /// Tests one explicit capability.
    pub fn has(&self, capability: &str) -> bool {
        self.capabilities.contains(capability)
    }
}

/// Result of compiling and committing JDL.
pub struct CompileCommit {
    pub output: CompileJdlOutput,
    pub idempotent_replay: bool,
}

/// Only service allowed to construct committed revisions and move project pointers.
pub struct Kernel {
    pub(crate) store: WorkspaceStore,
    pub(crate) registry: ExtensionRegistry,
    claims: ResourceClaimManager,
}

impl Kernel {
    /// Creates a kernel over an already-open workspace store.
    pub fn new(store: WorkspaceStore, registry: ExtensionRegistry) -> Self {
        Self {
            store,
            registry,
            claims: ResourceClaimManager::default(),
        }
    }

    /// Exposes read-only store APIs to the daemon's artifact and event routes.
    pub const fn store(&self) -> &WorkspaceStore {
        &self.store
    }

    /// Returns descriptors contributed by executable domain modules.
    pub fn extension_tool_descriptors(&self) -> Vec<jingwei_jdp::ToolDescriptor> {
        self.registry.tool_descriptors()
    }

    /// Executes a candidate-only domain operation without moving a revision pointer.
    pub async fn execute_domain_tool(
        &self,
        project_id: &ProjectId,
        revision: Option<&RevisionId>,
        operation: &str,
        input: Value,
        context: &RequestContext,
    ) -> Result<ToolResult, KernelError> {
        let module = self
            .registry
            .tool_module_for(operation)
            .ok_or_else(|| KernelError::CapabilityUnavailable(operation.to_owned()))?;
        let revision = match revision {
            Some(revision) => {
                self.store.revision(project_id, revision)?;
                revision.clone()
            }
            None => self.store.current_revision(project_id)?.ok_or_else(|| {
                KernelError::CorruptProject("project has no current revision".into())
            })?,
        };
        module
            .execute(
                ToolContext {
                    actor: context.actor.subject.clone(),
                    project_id: project_id.clone(),
                    revision,
                    capabilities: context.capabilities.iter().cloned().collect(),
                },
                ToolRequest {
                    name: operation.to_owned(),
                    input,
                },
            )
            .await
            .map_err(KernelError::DomainTool)
    }

    /// Creates project metadata and its deterministic empty initial revision.
    pub fn create_project(
        &self,
        project_id: ProjectId,
        name: String,
        request_id: RequestId,
        idempotency_key: IdempotencyKey,
        context: &RequestContext,
    ) -> Result<ProjectInfo, KernelError> {
        require(context, "design.write")?;
        let _claim = self.claim_project_write(&project_id, &request_id)?;
        let input_hash = jingwei_jir::canonical_sha256(&json!({"name": name}))?;
        if self.store.project_exists(&project_id) {
            if let Some(record) = self.store.idempotency(&project_id, &idempotency_key)? {
                ensure_replay(&record, None, &input_hash)?;
                return serde_json::from_value(record.response).map_err(Into::into);
            }
            return Err(KernelError::ProjectExists(project_id));
        }
        self.store.create_project(&ProjectMetadata {
            project_id: project_id.clone(),
            name: name.clone(),
            created_at: Utc::now(),
        })?;
        let project_entity = JirEntity {
            id: jingwei_jir::EntityId::new(format!("project-root-{project_id}"))?,
            kind: EntityKind::new("core.project")?,
            schema_version: jingwei_jir::JIR_SCHEMA_VERSION.into(),
            properties: CanonicalValue::new(json!({"name": name}))?,
            provenance: Provenance {
                source_kind: "project.create".into(),
                evidence_refs: Vec::new(),
                generator: Some("jingwei-kernel".into()),
                generator_version: Some(env!("CARGO_PKG_VERSION").into()),
            },
            edit_control: EditControl::default(),
        };
        let mut graph = JirGraph::default();
        graph.insert_entity(project_entity)?;
        let operation = json!({"name": name});
        let journal_hash = jingwei_jir::canonical_sha256(&operation)?;
        let snapshot =
            RevisionSnapshot::build(project_id.clone(), Vec::new(), graph, journal_hash)?;
        let output = ProjectInfo {
            project_id: project_id.clone(),
            name,
            current_revision: snapshot.metadata.id.clone(),
        };
        self.commit(
            snapshot,
            CommitMeta {
                request_id,
                idempotency_key,
                operation_name: "project.create",
                base_revision: None,
                input_hash,
                operations: operation,
                response: serde_json::to_value(&output)?,
                pointer_target: PointerTarget::ProjectHead,
                expected_pointer: None,
                proposal: None,
            },
            context,
        )?;
        Ok(output)
    }

    /// Returns project metadata and current pointer.
    pub fn project_info(
        &self,
        project_id: &ProjectId,
        context: &RequestContext,
    ) -> Result<ProjectInfo, KernelError> {
        require(context, "design.read")?;
        let project = self.store.project(project_id)?;
        let current_revision = self
            .store
            .current_revision(project_id)?
            .ok_or_else(|| KernelError::CorruptProject("project has no current revision".into()))?;
        Ok(ProjectInfo {
            project_id: project.project_id,
            name: project.name,
            current_revision,
        })
    }

    /// Applies an append-only change-proposal decision. Accept atomically moves project head.
    pub fn decide_change_proposal(
        &self,
        project_id: &ProjectId,
        input: ProposalDecisionInput,
        idempotency_key: &IdempotencyKey,
        context: &RequestContext,
    ) -> Result<ProposalDecisionOutput, KernelError> {
        require(context, "design.write")?;
        let decision_hash = jingwei_jir::canonical_sha256(&json!({
            "projectId": project_id,
            "proposalId": input.proposal_id,
            "action": input.action,
            "reason": input.reason.as_deref(),
            "actor": context.actor,
            "idempotencyKey": idempotency_key,
        }))?;
        let decision_id = DecisionId::new(format!("decision-{decision_hash}"))?;
        if let Some(existing) = self
            .store
            .proposal_decision(project_id, &input.proposal_id)?
        {
            if existing.id == decision_id && existing.action == input.action {
                let project_head = self.store.current_revision(project_id)?.ok_or_else(|| {
                    KernelError::CorruptProject("project has no current revision".into())
                })?;
                return Ok(ProposalDecisionOutput {
                    proposal_id: input.proposal_id,
                    action: input.action,
                    project_head,
                    idempotent_replay: true,
                });
            }
            return Err(KernelError::IdempotencyConflict);
        }
        let decision = ProposalDecisionRecord {
            id: decision_id,
            proposal_id: input.proposal_id.clone(),
            action: input.action,
            actor: context.actor.clone(),
            issued_at: Utc::now(),
            reason: input.reason,
        };
        self.store.decide_proposal(project_id, &decision)?;
        let project_head = self
            .store
            .current_revision(project_id)?
            .ok_or_else(|| KernelError::CorruptProject("project has no current revision".into()))?;
        Ok(ProposalDecisionOutput {
            proposal_id: input.proposal_id,
            action: input.action,
            project_head,
            idempotent_replay: false,
        })
    }

    /// Compiles, validates, and atomically commits JDL on an explicit base revision.
    #[allow(clippy::too_many_arguments)]
    pub async fn compile_jdl(
        &self,
        project_id: &ProjectId,
        base_revision: &RevisionId,
        source_name: &str,
        source: &str,
        request_id: RequestId,
        idempotency_key: IdempotencyKey,
        context: &RequestContext,
    ) -> Result<CompileCommit, KernelError> {
        require(context, "design.write")?;
        let _claim = self.claim_project_write(project_id, &request_id)?;
        let input = json!({"sourceName": source_name, "source": source});
        let input_hash = jingwei_jir::canonical_sha256(&input)?;
        if let Some(record) = self.store.idempotency(project_id, &idempotency_key)? {
            ensure_replay(&record, Some(base_revision), &input_hash)?;
            return Ok(CompileCommit {
                output: serde_json::from_value(record.response)?,
                idempotent_replay: true,
            });
        }
        let current = self.current_snapshot(project_id)?;
        if &current.metadata.id != base_revision {
            return Err(KernelError::StaleBase {
                expected: base_revision.clone(),
                actual: current.metadata.id,
            });
        }
        let document =
            jingwei_jdl::parse(source_name, source, jingwei_jdl::ParseLimits::default())?;
        let formatted_source = jingwei_jdl::format(&document);
        let compiled = jingwei_jdl::compile(&document)?;
        let validation = self
            .validate_graph(
                base_revision.clone(),
                &compiled.graph,
                Vec::new(),
                Vec::new(),
            )
            .await;
        if !validation.valid {
            return Err(KernelError::Postcondition(
                "compiled graph failed validation".into(),
            ));
        }
        let operation = json!({"kind": "compile-jdl", "inputHash": input_hash});
        let journal_hash = jingwei_jir::canonical_sha256(&operation)?;
        let snapshot = RevisionSnapshot::build(
            project_id.clone(),
            vec![base_revision.clone()],
            compiled.graph,
            journal_hash,
        )?;
        let proposal_id = ProposalId::generate();
        let output = CompileJdlOutput {
            revision: snapshot.metadata.id.clone(),
            proposal_id: proposal_id.clone(),
            entity_count: snapshot.graph.entities.len(),
            edge_count: snapshot.graph.edges.len(),
            formatted_source,
            diagnostics: compiled
                .diagnostics
                .into_iter()
                .map(|diagnostic| ProtocolDiagnostic {
                    severity: format!("{:?}", diagnostic.severity).to_ascii_lowercase(),
                    code: diagnostic.code,
                    message: diagnostic.message,
                    source_name: diagnostic.source_name,
                    start: diagnostic.span.start,
                    end: diagnostic.span.end,
                })
                .collect(),
        };
        self.commit(
            snapshot,
            CommitMeta {
                request_id,
                idempotency_key,
                operation_name: "design.compile_jdl",
                base_revision: Some(base_revision.clone()),
                input_hash,
                operations: operation,
                response: serde_json::to_value(&output)?,
                pointer_target: PointerTarget::None,
                expected_pointer: None,
                proposal: Some(ProposalMeta {
                    id: proposal_id,
                    base_project_head: base_revision.clone(),
                }),
            },
            context,
        )?;
        Ok(CompileCommit {
            output,
            idempotent_replay: false,
        })
    }

    /// Validates and previews an atomic patch without writing JIR.
    pub async fn propose_patch(
        &self,
        project_id: &ProjectId,
        base_revision: &RevisionId,
        patch: &PatchInput,
        context: &RequestContext,
    ) -> Result<PatchProposal, KernelError> {
        require(context, "design.write")?;
        let snapshot = self.store.revision(project_id, base_revision)?;
        evaluate_preconditions(&snapshot.graph, &patch.preconditions, context)?;
        let applied = apply_patch_to_graph(&snapshot.graph, &patch.operations, context)?;
        let validation = self
            .validate_graph(
                base_revision.clone(),
                &applied.graph,
                applied.impacted_entities,
                Vec::new(),
            )
            .await;
        let normalized_input_hash = jingwei_jir::canonical_sha256(patch)?;
        let proposal_hash = jingwei_jir::canonical_sha256(&json!({
            "base": base_revision,
            "input": normalized_input_hash,
        }))?;
        Ok(PatchProposal {
            proposal_id: format!("proposal-{}", &proposal_hash[..24]),
            base_revision: base_revision.clone(),
            normalized_input_hash,
            diff: snapshot.graph.diff(&applied.graph),
            validation,
        })
    }

    /// Applies a validated patch and commits exactly one new revision.
    #[allow(clippy::too_many_arguments)]
    pub async fn apply_patch(
        &self,
        project_id: &ProjectId,
        base_revision: &RevisionId,
        patch: &PatchInput,
        request_id: RequestId,
        idempotency_key: IdempotencyKey,
        context: &RequestContext,
    ) -> Result<PatchOutput, KernelError> {
        require(context, "design.write")?;
        let _claim = self.claim_project_write(project_id, &request_id)?;
        let input_hash = jingwei_jir::canonical_sha256(patch)?;
        if let Some(record) = self.store.idempotency(project_id, &idempotency_key)? {
            ensure_replay(&record, Some(base_revision), &input_hash)?;
            let mut output: PatchOutput = serde_json::from_value(record.response)?;
            output.idempotent_replay = true;
            return Ok(output);
        }
        let current = self.current_snapshot(project_id)?;
        if &current.metadata.id != base_revision {
            return Err(KernelError::StaleBase {
                expected: base_revision.clone(),
                actual: current.metadata.id,
            });
        }
        evaluate_preconditions(&current.graph, &patch.preconditions, context)?;
        let applied = apply_patch_to_graph(&current.graph, &patch.operations, context)?;
        let validation = self
            .validate_graph(
                base_revision.clone(),
                &applied.graph,
                applied.impacted_entities,
                Vec::new(),
            )
            .await;
        if !validation.valid {
            return Err(KernelError::Postcondition("patch validation failed".into()));
        }
        for condition in &patch.expected_postconditions {
            if condition != "graph.integrity" {
                return Err(KernelError::Postcondition(format!(
                    "unsupported postcondition `{condition}`"
                )));
            }
        }
        let journal_operations = json!({
            "forward": patch.operations,
            "inverse": applied.inverse,
            "preconditions": patch.preconditions,
            "postconditions": patch.expected_postconditions,
        });
        let journal_hash = jingwei_jir::canonical_sha256(&journal_operations)?;
        let snapshot = RevisionSnapshot::build(
            project_id.clone(),
            vec![base_revision.clone()],
            applied.graph,
            journal_hash,
        )?;
        let proposal_id = ProposalId::generate();
        let output = PatchOutput {
            revision: snapshot.metadata.id.clone(),
            proposal_id: proposal_id.clone(),
            diff: current.graph.diff(&snapshot.graph),
            validation,
            idempotent_replay: false,
        };
        self.commit(
            snapshot,
            CommitMeta {
                request_id,
                idempotency_key,
                operation_name: "design.patch.apply",
                base_revision: Some(base_revision.clone()),
                input_hash,
                operations: journal_operations,
                response: serde_json::to_value(&output)?,
                pointer_target: PointerTarget::None,
                expected_pointer: None,
                proposal: Some(ProposalMeta {
                    id: proposal_id,
                    base_project_head: base_revision.clone(),
                }),
            },
            context,
        )?;
        Ok(output)
    }

    /// Performs a bounded entity/edge query.
    pub fn query(
        &self,
        project_id: &ProjectId,
        input: &QueryInput,
        context: &RequestContext,
    ) -> Result<QueryOutput, KernelError> {
        require(context, "design.read")?;
        let revision = match &input.revision {
            Some(revision) => revision.clone(),
            None => self.store.current_revision(project_id)?.ok_or_else(|| {
                KernelError::CorruptProject("project has no current revision".into())
            })?,
        };
        let snapshot = self.store.revision(project_id, &revision)?;
        let offset = input
            .cursor
            .as_deref()
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| {
                KernelError::InvalidQuery(
                    "cursor must be a decimal offset returned by the daemon".into(),
                )
            })?;
        let limit = input.limit.clamp(1, 1000);
        let mut entities = snapshot
            .graph
            .entities
            .values()
            .filter(|entity| {
                input
                    .kind
                    .as_ref()
                    .is_none_or(|kind| entity.kind.as_str() == kind)
            })
            .filter(|entity| input.ids.is_empty() || input.ids.contains(&entity.id))
            .skip(offset)
            .take(limit + 1)
            .cloned()
            .collect::<Vec<_>>();
        let has_more = entities.len() > limit;
        entities.truncate(limit);
        let selected: BTreeSet<_> = entities.iter().map(|entity| entity.id.clone()).collect();
        let edges = snapshot
            .graph
            .edges
            .values()
            .filter(|edge| selected.contains(&edge.from) || selected.contains(&edge.to))
            .cloned()
            .collect();
        Ok(QueryOutput {
            revision,
            entities,
            edges,
            next_cursor: has_more.then(|| (offset + limit).to_string()),
        })
    }

    /// Explains one entity and its direct relations.
    pub fn explain(
        &self,
        project_id: &ProjectId,
        revision: Option<&RevisionId>,
        entity_id: &jingwei_jir::EntityId,
        context: &RequestContext,
    ) -> Result<ExplainOutput, KernelError> {
        require(context, "design.read")?;
        let revision = revision
            .cloned()
            .or(self.store.current_revision(project_id)?)
            .ok_or_else(|| KernelError::CorruptProject("project has no current revision".into()))?;
        let snapshot = self.store.revision(project_id, &revision)?;
        let entity = snapshot
            .graph
            .entities
            .get(entity_id)
            .cloned()
            .ok_or_else(|| jingwei_jir::GraphError::MissingEntity(entity_id.clone()))?;
        let incoming = snapshot
            .graph
            .edges
            .values()
            .filter(|edge| &edge.to == entity_id)
            .cloned()
            .collect();
        let outgoing = snapshot
            .graph
            .edges
            .values()
            .filter(|edge| &edge.from == entity_id)
            .cloned()
            .collect();
        let explanation = format!(
            "{} was produced from {} by {} and carries {} evidence reference(s)",
            entity.kind.as_str(),
            entity.provenance.source_kind,
            entity
                .provenance
                .generator
                .as_deref()
                .unwrap_or("an unspecified generator"),
            entity.provenance.evidence_refs.len()
        );
        Ok(ExplainOutput {
            entity,
            incoming,
            outgoing,
            explanation,
        })
    }

    /// Computes a semantic graph diff between immutable revisions.
    pub fn diff(
        &self,
        project_id: &ProjectId,
        left: &RevisionId,
        right: &RevisionId,
        context: &RequestContext,
    ) -> Result<DiffOutput, KernelError> {
        require(context, "design.read")?;
        let left_snapshot = self.store.revision(project_id, left)?;
        let right_snapshot = self.store.revision(project_id, right)?;
        Ok(DiffOutput {
            left: left.clone(),
            right: right.clone(),
            diff: left_snapshot.graph.diff(&right_snapshot.graph),
        })
    }

    /// Runs core integrity and registered validators.
    pub async fn validate(
        &self,
        project_id: &ProjectId,
        input: &ValidateInput,
        context: &RequestContext,
    ) -> Result<ValidationOutput, KernelError> {
        require(context, "design.read")?;
        let revision = input
            .revision
            .clone()
            .or(self.store.current_revision(project_id)?)
            .ok_or_else(|| KernelError::CorruptProject("project has no current revision".into()))?;
        let snapshot = self.store.revision(project_id, &revision)?;
        Ok(self
            .validate_graph(revision, &snapshot.graph, Vec::new(), input.scopes.clone())
            .await)
    }

    fn current_snapshot(&self, project_id: &ProjectId) -> Result<RevisionSnapshot, KernelError> {
        let revision = self
            .store
            .current_revision(project_id)?
            .ok_or_else(|| KernelError::CorruptProject("project has no current revision".into()))?;
        self.store
            .revision(project_id, &revision)
            .map_err(Into::into)
    }

    pub(crate) fn claim_project_write(
        &self,
        project_id: &ProjectId,
        owner: &RequestId,
    ) -> Result<ClaimGuard, KernelError> {
        self.claims
            .acquire(
                owner.to_string(),
                vec![ResourceClaim {
                    resource: format!("project:{project_id}"),
                    mode: ClaimMode::Write,
                }],
            )
            .map_err(Into::into)
    }

    async fn validate_graph(
        &self,
        revision: RevisionId,
        graph: &JirGraph,
        impacted_entities: Vec<jingwei_jir::EntityId>,
        scopes: Vec<String>,
    ) -> ValidationOutput {
        let mut output = ValidationOutput {
            valid: graph.validate().is_ok(),
            findings: Vec::new(),
            providers_run: vec!["core.graph-integrity".into()],
        };
        if !output.valid {
            output.findings.push(ValidationFinding {
                code: "JIR_GRAPH_INVALID".into(),
                severity: "error".into(),
                summary: "graph integrity validation failed".into(),
                object_refs: Vec::new(),
                fingerprint: "core-graph-invalid".into(),
            });
        }
        let validation_context = ValidationContext {
            revision,
            impacted_entities,
            scopes,
        };
        for (name, provider) in &self.registry.validators {
            let provider_output = provider.validate(graph, &validation_context).await;
            output.valid &= provider_output.valid;
            output.findings.extend(provider_output.findings);
            output.providers_run.push(name.clone());
        }
        output
    }

    pub(crate) fn commit(
        &self,
        snapshot: RevisionSnapshot,
        meta: CommitMeta<'_>,
        context: &RequestContext,
    ) -> Result<(), KernelError> {
        let execution_id = ExecutionId::generate();
        let commit_id = CommitId::generate();
        let issued_at = Utc::now();
        let correlation = InvocationCorrelation {
            invocation_id: InvocationId::generate(),
            source_kind: context.invocation_source,
            source_id: context.invocation_source_id.clone(),
            request_id: meta.request_id.clone(),
            daemon_instance_id: context.daemon_instance_id.clone(),
            daemon_epoch: context.daemon_epoch,
        };
        let event_sequence = self
            .store
            .next_event_sequence(&snapshot.metadata.project_id)?;
        let project_id = snapshot.metadata.project_id.clone();
        let result_revision = snapshot.metadata.id.clone();
        let proposal = meta.proposal.map(|proposal| ChangeProposal {
            id: proposal.id,
            project_id: project_id.clone(),
            base_project_head: proposal.base_project_head,
            proposed_revision: result_revision.clone(),
            created_by_commit: commit_id.clone(),
            issued_at,
        });
        self.store.commit(CommitRequest {
            snapshot,
            commit_record: RevisionCommitRecord {
                id: commit_id.clone(),
                project_id: project_id.clone(),
                revision_id: result_revision.clone(),
                actor: context.actor.clone(),
                issued_at,
                correlation: correlation.clone(),
            },
            proposal,
            journal: OperationJournalRecord {
                execution_id: execution_id.clone(),
                request_id: meta.request_id,
                project_id: project_id.clone(),
                idempotency_key: meta.idempotency_key.clone(),
                operation: meta.operation_name.into(),
                base_revision: meta.base_revision.clone(),
                result_revision: result_revision.clone(),
                normalized_input_hash: meta.input_hash.clone(),
                commit_id,
                correlation,
                operations: meta.operations,
                artifact_refs: Vec::new(),
                evidence_refs: Vec::new(),
            },
            idempotency: IdempotencyRecord {
                key: meta.idempotency_key,
                project_id: project_id.clone(),
                base_revision: meta.base_revision,
                normalized_input_hash: meta.input_hash,
                result_revision: result_revision.clone(),
                response: meta.response,
            },
            event: StoredEvent {
                aggregate_id: project_id.to_string(),
                sequence: event_sequence,
                timestamp: Utc::now(),
                event_type: "revision.committed".into(),
                data: json!({
                    "executionId": execution_id,
                    "revision": result_revision,
                    "operation": meta.operation_name,
                }),
            },
            pointer_target: meta.pointer_target,
            expected_pointer: meta.expected_pointer,
        })?;
        Ok(())
    }
}

pub(crate) struct CommitMeta<'a> {
    pub(crate) request_id: RequestId,
    pub(crate) idempotency_key: IdempotencyKey,
    pub(crate) operation_name: &'a str,
    pub(crate) base_revision: Option<RevisionId>,
    pub(crate) input_hash: String,
    pub(crate) operations: Value,
    pub(crate) response: Value,
    pub(crate) pointer_target: PointerTarget,
    pub(crate) expected_pointer: Option<RevisionId>,
    pub(crate) proposal: Option<ProposalMeta>,
}

pub(crate) struct ProposalMeta {
    pub(crate) id: ProposalId,
    pub(crate) base_project_head: RevisionId,
}

fn require(context: &RequestContext, capability: &str) -> Result<(), KernelError> {
    if context.has(capability) {
        Ok(())
    } else {
        Err(KernelError::PermissionDenied(capability.into()))
    }
}

fn ensure_replay(
    record: &IdempotencyRecord,
    base_revision: Option<&RevisionId>,
    input_hash: &str,
) -> Result<(), KernelError> {
    if record.base_revision.as_ref() == base_revision && record.normalized_input_hash == input_hash
    {
        Ok(())
    } else {
        Err(KernelError::IdempotencyConflict)
    }
}

/// Kernel validation, authorization, transaction, or persistence failure.
#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("project already exists: {0}")]
    ProjectExists(ProjectId),
    #[error("stale base revision: expected {expected}, current is {actual}")]
    StaleBase {
        expected: RevisionId,
        actual: RevisionId,
    },
    #[error("idempotency key was already used with different normalized input or base")]
    IdempotencyConflict,
    #[error("precondition failed: {0}")]
    Precondition(String),
    #[error("postcondition failed: {0}")]
    Postcondition(String),
    #[error("invalid patch: {0}")]
    InvalidPatch(String),
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("capability unavailable: {0}")]
    CapabilityUnavailable(String),
    #[error("domain tool failed: {0}")]
    DomainTool(String),
    #[error("project storage is inconsistent: {0}")]
    CorruptProject(String),
    #[error(transparent)]
    Store(#[from] jingwei_store::StoreError),
    #[error(transparent)]
    Graph(#[from] jingwei_jir::GraphError),
    #[error(transparent)]
    Canonical(#[from] jingwei_jir::CanonicalError),
    #[error(transparent)]
    InvalidId(#[from] jingwei_jir::InvalidId),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Parse(#[from] jingwei_jdl::ParseError),
    #[error(transparent)]
    Compile(#[from] jingwei_jdl::CompileError),
    #[error(transparent)]
    ResourceConflict(#[from] crate::claims::ResourceConflict),
}
