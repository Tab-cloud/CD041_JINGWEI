use async_trait::async_trait;
use jingwei_components::{
    CandidateEvaluation, DesignIntent, LocalCatalogAdapter, PartCandidate, PartRequirement,
    PartSourceAdapter, classify_risk, compare_candidates, select_part, validate_substitution,
};
use jingwei_erc::{ErcContext, LibraryBindingStatus, run_native_erc};
use jingwei_jdp::{
    DisplayList, Effect, HitTestRequest, HitTestResult, PatchOperation, Permission,
    SemanticEditCommand, ToolDescriptor, ValidationFinding, ValidationOutput, ViewRequest,
};
use jingwei_jir::{
    CanonicalValue, EditControl, EntityId, EntityKind, JirEntity, Provenance, SchemaDescriptor,
};
use jingwei_kernel::{
    DomainToolModule, ExtensionRegistryBuilder, ProjectionArtifact, ProjectionBackend,
    ProjectionRequest, ProjectionResult, RegistryError, SemanticEditHandler, ToolContext,
    ToolRequest, ToolResult, ValidationContext, ValidationProvider, ViewProvider,
};
use jingwei_library::{
    GovernanceScope, LibraryAssetCandidate, LibraryGovernanceLedger, LibraryGovernancePolicy,
    LibraryPolicy, LibraryUseClass, ProviderSnapshot, evaluate_candidate_governance,
    resolve_library, verify_library,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Registration bundle consumed by the core registry adapter once available.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicalModuleRegistration {
    pub module_name: String,
    pub module_version: String,
    pub entity_schema_kinds: Vec<String>,
    pub edge_schema_kinds: Vec<String>,
    pub tools: Vec<ToolDescriptor>,
    pub validation_providers: Vec<String>,
    pub view_providers: Vec<String>,
    pub projection_backends: Vec<String>,
    pub integration_state: String,
}

/// Returns all Task 03 tools without claiming revision ownership.
#[must_use]
pub fn logical_module_registration() -> LogicalModuleRegistration {
    let definitions = [
        (
            "requirement_compile",
            Permission::CandidateWrite,
            Effect::ReadOnly,
        ),
        (
            "requirement_baseline",
            Permission::CandidateWrite,
            Effect::RevisionWrite,
        ),
        (
            "requirement_change_analyze",
            Permission::Read,
            Effect::ReadOnly,
        ),
        ("risk_classify", Permission::Read, Effect::ReadOnly),
        ("part_search", Permission::Read, Effect::ReadOnly),
        ("part_compare", Permission::Read, Effect::ReadOnly),
        (
            "part_select",
            Permission::CandidateWrite,
            Effect::RevisionWrite,
        ),
        (
            "part_substitute_validate",
            Permission::Read,
            Effect::ReadOnly,
        ),
        (
            "datasheet_extract",
            Permission::CandidateWrite,
            Effect::ReadOnly,
        ),
        ("library_resolve", Permission::Read, Effect::ReadOnly),
        (
            "symbol_generate",
            Permission::CandidateWrite,
            Effect::RevisionWrite,
        ),
        (
            "footprint_generate",
            Permission::CandidateWrite,
            Effect::RevisionWrite,
        ),
        ("library_verify", Permission::Read, Effect::ReadOnly),
        (
            "library_governance_evaluate",
            Permission::Read,
            Effect::ReadOnly,
        ),
        ("library_diff", Permission::Read, Effect::ReadOnly),
        (
            "schematic_create",
            Permission::CandidateWrite,
            Effect::RevisionWrite,
        ),
        (
            "schematic_apply_change",
            Permission::CandidateWrite,
            Effect::RevisionWrite,
        ),
        (
            "schematic_connect",
            Permission::CandidateWrite,
            Effect::RevisionWrite,
        ),
        ("schematic_erc", Permission::Read, Effect::ReadOnly),
        (
            "schematic_verify_custom",
            Permission::Read,
            Effect::ReadOnly,
        ),
        (
            "schematic_render",
            Permission::ExternalProcess,
            Effect::ReadOnly,
        ),
        (
            "schematic_project",
            Permission::ExternalProcess,
            Effect::ReadOnly,
        ),
    ];
    let tools = definitions
        .into_iter()
        .map(|(name, permission, effect)| ToolDescriptor {
            name: name.to_owned(),
            description: logical_tool_description(name).to_owned(),
            // The core registry does not yet expose per-handler schema registration.
            // Keep this permissive until the proposed executable adapter can attach
            // each operation's concrete request schema.
            input_schema: json!({"type": "object"}),
            output_schema: json!({"type": "object"}),
            permission,
            effect,
            resource_claim_template: vec![
                "workspace:{workspaceId}".to_owned(),
                "project:{projectId}".to_owned(),
                "logical:{projectId}".to_owned(),
            ],
            idempotent: true,
            requires_base_revision: matches!(effect, Effect::RevisionWrite | Effect::DraftWrite),
            cancellable: false,
            default_timeout_ms: 30_000,
            protocol_version: jingwei_jdp::JDP_PROTOCOL_VERSION.to_owned(),
            schema_version: jingwei_schematic::SCHEMATIC_SCHEMA_VERSION.to_owned(),
        })
        .collect();
    LogicalModuleRegistration {
        module_name: "jingwei-logical-domain".to_owned(),
        module_version: env!("CARGO_PKG_VERSION").to_owned(),
        entity_schema_kinds: [
            "logical.design-intent",
            "logical.part",
            "logical.library-asset",
            "logical.schematic",
            "logical.sheet",
            "logical.module",
            "logical.symbol",
            "logical.pin",
            "logical.net",
            "logical.test-point",
            "logical.finding",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        edge_schema_kinds: [
            "logical.contains",
            "logical.connects",
            "logical.implements",
            "logical.verified-by",
            "logical.projected-to",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        tools,
        validation_providers: vec!["jingwei-native-erc@0.1".to_owned()],
        view_providers: vec!["jingwei-schematic-view@0.1".to_owned()],
        projection_backends: vec!["kicad-schematic-model@0.1[target=10.0.5]".to_owned()],
        integration_state: "candidate-only; kernel registry adapter pending".to_owned(),
    }
}

/// Registers the executable logical-domain adapters into the Task 02 registry.
pub fn register_logical_module(
    mut builder: ExtensionRegistryBuilder,
) -> Result<ExtensionRegistryBuilder, RegistryError> {
    let metadata = logical_module_registration();
    builder = builder.tool_module(LogicalToolModule)?;
    builder = builder.validator(LogicalGraphValidator)?;
    builder = builder.view_provider(LogicalViewProvider)?;
    builder = builder.projection(LogicalProjectionBackend)?;
    builder = builder.edit_handler(LogicalSetPropertyHandler)?;
    builder = builder.edit_handler(LogicalMoveSymbolHandler)?;
    for kind in metadata.entity_schema_kinds {
        builder = builder.entity_schema(open_schema(kind))?;
    }
    for kind in metadata.edge_schema_kinds {
        builder = builder.edge_schema(open_schema(kind))?;
    }
    Ok(builder)
}

fn open_schema(kind: String) -> SchemaDescriptor {
    SchemaDescriptor {
        kind,
        schema_version: jingwei_schematic::SCHEMATIC_SCHEMA_VERSION.to_owned(),
        json_schema: json!({"type": "object", "additionalProperties": true}),
    }
}

struct LogicalToolModule;

#[async_trait]
impl DomainToolModule for LogicalToolModule {
    fn module_name(&self) -> &'static str {
        "jingwei-logical-domain"
    }

    fn descriptors(&self) -> Vec<ToolDescriptor> {
        logical_module_registration()
            .tools
            .into_iter()
            .filter(|descriptor| {
                matches!(
                    descriptor.name.as_str(),
                    "requirement_compile"
                        | "risk_classify"
                        | "part_search"
                        | "part_compare"
                        | "part_select"
                        | "part_substitute_validate"
                        | "library_resolve"
                        | "library_verify"
                        | "library_governance_evaluate"
                        | "schematic_create"
                )
            })
            .collect()
    }

    async fn execute(
        &self,
        _context: ToolContext,
        request: ToolRequest,
    ) -> Result<ToolResult, String> {
        let data = match request.name.as_str() {
            "requirement_compile" => {
                let intent: DesignIntent = serde_json::from_value(request.input)
                    .map_err(|error| format!("invalid DesignIntent: {error}"))?;
                serde_json::to_value(
                    intent
                        .compile_candidate()
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?
            }
            "risk_classify" => {
                let intent: DesignIntent = serde_json::from_value(request.input)
                    .map_err(|error| format!("invalid DesignIntent: {error}"))?;
                serde_json::to_value(classify_risk(&intent)).map_err(|error| error.to_string())?
            }
            "part_search" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Input {
                    requirement: PartRequirement,
                    candidates: Vec<PartCandidate>,
                    snapshot_id: String,
                }
                let input: Input = serde_json::from_value(request.input)
                    .map_err(|error| format!("invalid offline part search input: {error}"))?;
                let adapter = LocalCatalogAdapter::new(input.snapshot_id, input.candidates);
                serde_json::to_value(
                    adapter
                        .search(&input.requirement)
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?
            }
            "part_compare" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Input {
                    requirement: PartRequirement,
                    candidates: Vec<PartCandidate>,
                }
                let input: Input = serde_json::from_value(request.input)
                    .map_err(|error| format!("invalid part comparison input: {error}"))?;
                serde_json::to_value(compare_candidates(&input.requirement, input.candidates))
                    .map_err(|error| error.to_string())?
            }
            "part_select" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Input {
                    requirement: PartRequirement,
                    evaluations: Vec<CandidateEvaluation>,
                }
                let input: Input = serde_json::from_value(request.input)
                    .map_err(|error| format!("invalid part selection input: {error}"))?;
                let decision = select_part(&input.requirement, input.evaluations)
                    .map_err(|error| error.to_string())?;
                let entity = JirEntity {
                    id: EntityId::new(decision.id.clone()).map_err(|error| error.to_string())?,
                    kind: EntityKind::new("logical.part").map_err(|error| error.to_string())?,
                    schema_version: jingwei_schematic::SCHEMATIC_SCHEMA_VERSION.to_owned(),
                    properties: CanonicalValue::new(
                        serde_json::to_value(&decision).map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?,
                    provenance: Provenance {
                        source_kind: "part-selection".to_owned(),
                        evidence_refs: decision.selected.evidence_refs.clone(),
                        generator: Some("jingwei-components".to_owned()),
                        generator_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
                    },
                    edit_control: EditControl::default(),
                };
                return Ok(ToolResult {
                    candidate_operations: vec![PatchOperation::AddEntity { entity }],
                    data: serde_json::to_value(decision).map_err(|error| error.to_string())?,
                    artifact_refs: Vec::new(),
                    evidence_refs: Vec::new(),
                });
            }
            "part_substitute_validate" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Input {
                    requirement: PartRequirement,
                    primary: PartCandidate,
                    substitute: PartCandidate,
                    pinout_exact: bool,
                }
                let input: Input = serde_json::from_value(request.input)
                    .map_err(|error| format!("invalid part substitution input: {error}"))?;
                serde_json::to_value(validate_substitution(
                    &input.requirement,
                    &input.primary,
                    &input.substitute,
                    input.pinout_exact,
                ))
                .map_err(|error| error.to_string())?
            }
            "library_resolve" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Input {
                    mpn: jingwei_components::ExactMpn,
                    package: jingwei_components::PackageRef,
                    policy: LibraryPolicy,
                    candidates: Vec<LibraryAssetCandidate>,
                }
                let input: Input = serde_json::from_value(request.input)
                    .map_err(|error| format!("invalid library resolution input: {error}"))?;
                serde_json::to_value(resolve_library(
                    &input.mpn,
                    &input.package,
                    &input.policy,
                    &input.candidates,
                ))
                .map_err(|error| error.to_string())?
            }
            "library_verify" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Input {
                    candidate: LibraryAssetCandidate,
                }
                let input: Input = serde_json::from_value(request.input)
                    .map_err(|error| format!("invalid library verification input: {error}"))?;
                serde_json::to_value(verify_library(&input.candidate))
                    .map_err(|error| error.to_string())?
            }
            "library_governance_evaluate" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Input {
                    candidate: LibraryAssetCandidate,
                    snapshots: Vec<ProviderSnapshot>,
                    ledger: LibraryGovernanceLedger,
                    policy: LibraryGovernancePolicy,
                    scopes: Vec<GovernanceScope>,
                    use_class: LibraryUseClass,
                    now_epoch_seconds: u64,
                }
                let input: Input = serde_json::from_value(request.input)
                    .map_err(|error| format!("invalid library governance input: {error}"))?;
                serde_json::to_value(evaluate_candidate_governance(
                    &input.candidate,
                    &input.snapshots,
                    &input.ledger,
                    &input.policy,
                    &input.scopes,
                    input.use_class,
                    input.now_epoch_seconds,
                ))
                .map_err(|error| error.to_string())?
            }
            "schematic_create" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Input {
                    graph: jingwei_schematic::SchematicGraph,
                }
                let input: Input = serde_json::from_value(request.input)
                    .map_err(|error| format!("invalid SchematicGraph input: {error}"))?;
                let graph = input
                    .graph
                    .to_jir_graph()
                    .map_err(|error| error.to_string())?;
                let entity_count = graph.entities.len();
                let edge_count = graph.edges.len();
                return Ok(ToolResult {
                    candidate_operations: graph
                        .entities
                        .into_values()
                        .map(|entity| PatchOperation::AddEntity { entity })
                        .chain(
                            graph
                                .edges
                                .into_values()
                                .map(|edge| PatchOperation::AddEdge { edge }),
                        )
                        .collect(),
                    data: json!({
                        "graphId": input.graph.id,
                        "entityCount": entity_count,
                        "edgeCount": edge_count,
                        "candidateOnly": true,
                    }),
                    artifact_refs: Vec::new(),
                    evidence_refs: input
                        .graph
                        .symbols
                        .values()
                        .flat_map(|symbol| symbol.evidence_refs.iter().cloned())
                        .collect(),
                });
            }
            _ => {
                return Err(format!(
                    "logical operation is not executable: {}",
                    request.name
                ));
            }
        };
        Ok(ToolResult {
            candidate_operations: Vec::new(),
            data,
            artifact_refs: Vec::new(),
            evidence_refs: Vec::new(),
        })
    }
}

struct LogicalGraphValidator;

#[async_trait]
impl ValidationProvider for LogicalGraphValidator {
    fn name(&self) -> &'static str {
        "logical.namespaced-graph"
    }

    async fn validate(
        &self,
        graph: &jingwei_jir::JirGraph,
        _context: &ValidationContext,
    ) -> ValidationOutput {
        if !graph
            .entities
            .values()
            .any(|entity| entity.kind.as_str() == "logical.schematic")
        {
            return ValidationOutput {
                valid: true,
                findings: Vec::new(),
                providers_run: vec![self.name().to_owned()],
            };
        }
        let schematic = match jingwei_schematic::SchematicGraph::from_jir_graph(graph) {
            Ok(schematic) => schematic,
            Err(error) => {
                return ValidationOutput {
                    valid: false,
                    findings: vec![ValidationFinding {
                        code: "SCH.JIR_ROUND_TRIP".into(),
                        severity: "blocking".into(),
                        summary: error.to_string(),
                        object_refs: Vec::new(),
                        fingerprint: "schematic-jir-round-trip".into(),
                    }],
                    providers_run: vec![self.name().to_owned()],
                };
            }
        };
        let library_bindings = schematic
            .symbols
            .values()
            .map(|symbol| {
                let status = if symbol
                    .properties
                    .get("library.verification")
                    .map(String::as_str)
                    == Some("verified-exact")
                {
                    LibraryBindingStatus::VerifiedExact
                } else {
                    LibraryBindingStatus::Unverified
                };
                (symbol.id.clone(), status)
            })
            .collect();
        let mut output = run_native_erc(
            &schematic,
            &ErcContext {
                library_bindings,
                project_risk_profile: "low-voltage-general".into(),
            },
        )
        .to_jdp_validation();
        output.providers_run.insert(0, self.name().to_owned());
        output
    }
}

struct LogicalViewProvider;

#[async_trait]
impl ViewProvider for LogicalViewProvider {
    fn name(&self) -> &'static str {
        "logical.schematic-view"
    }

    async fn view(
        &self,
        graph: &jingwei_jir::JirGraph,
        request: &ViewRequest,
    ) -> Result<DisplayList, String> {
        if !graph
            .entities
            .values()
            .any(|entity| entity.kind.as_str() == "logical.schematic")
        {
            return Ok(DisplayList {
                revision: request.revision.clone(),
                primitives: Vec::new(),
                artifact_refs: Vec::new(),
            });
        }
        let schematic = jingwei_schematic::SchematicGraph::from_jir_graph(graph)
            .map_err(|error| error.to_string())?;
        Ok(jingwei_schematic::display_list(&schematic, request))
    }

    async fn hit_test(
        &self,
        graph: &jingwei_jir::JirGraph,
        request: &HitTestRequest,
    ) -> Result<HitTestResult, String> {
        if !graph
            .entities
            .values()
            .any(|entity| entity.kind.as_str() == "logical.schematic")
        {
            return Ok(HitTestResult { hits: Vec::new() });
        }
        let schematic = jingwei_schematic::SchematicGraph::from_jir_graph(graph)
            .map_err(|error| error.to_string())?;
        Ok(jingwei_schematic::hit_test(&schematic, request))
    }
}

struct LogicalProjectionBackend;

#[async_trait]
impl ProjectionBackend for LogicalProjectionBackend {
    fn name(&self) -> &'static str {
        "logical.kicad-schematic"
    }

    async fn project(
        &self,
        graph: &jingwei_jir::JirGraph,
        request: &ProjectionRequest,
    ) -> Result<ProjectionResult, String> {
        let schematic = jingwei_schematic::SchematicGraph::from_jir_graph(graph)
            .map_err(|error| error.to_string())?;
        let model = crate::project_schematic(&schematic).map_err(|error| error.to_string())?;
        let projection = crate::serialize_projection(&model).map_err(|error| error.to_string())?;
        let native = crate::build_native_artifact(&model).map_err(|error| error.to_string())?;
        Ok(ProjectionResult {
            artifacts: vec![
                ProjectionArtifact {
                    name: "design.kicad_sch".into(),
                    media_type: "application/x-kicad-schematic".into(),
                    bytes: native.schematic,
                },
                ProjectionArtifact {
                    name: "design.object-map.json".into(),
                    media_type: "application/json".into(),
                    bytes: native.object_map,
                },
                ProjectionArtifact {
                    name: crate::KICAD_NATIVE_SYMBOL_LIBRARY_FILE.into(),
                    media_type: "application/x-kicad-symbol-library".into(),
                    bytes: native.symbol_library,
                },
                ProjectionArtifact {
                    name: "sym-lib-table".into(),
                    media_type: "text/plain".into(),
                    bytes: native.symbol_library_table,
                },
                ProjectionArtifact {
                    name: "design.projection.json".into(),
                    media_type: "application/json".into(),
                    bytes: projection,
                },
            ],
            artifact_refs: Vec::new(),
            evidence_refs: Vec::new(),
            mapping: json!({
                "backend": request.backend,
                "revision": request.revision,
                "sourceGraphId": model.source_graph_id,
                "targetKiCadVersion": model.target_kicad_version,
                "coverage": model.coverage,
                "nativeSummary": native.summary,
            }),
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetPropertyPayload {
    entity_id: String,
    key: String,
    value: Value,
}

struct LogicalSetPropertyHandler;

impl SemanticEditHandler for LogicalSetPropertyHandler {
    fn command_kind(&self) -> &'static str {
        "logical.set-property"
    }

    fn translate(
        &self,
        graph: &jingwei_jir::JirGraph,
        command: &SemanticEditCommand,
    ) -> Result<Vec<PatchOperation>, String> {
        let payload: SetPropertyPayload = serde_json::from_value(command.payload.clone())
            .map_err(|error| format!("invalid logical set-property payload: {error}"))?;
        let entity_id = EntityId::new(payload.entity_id).map_err(|error| error.to_string())?;
        if !graph.entities.contains_key(&entity_id) {
            return Err(format!("logical entity does not exist: {entity_id}"));
        }
        Ok(vec![PatchOperation::SetProperty {
            entity_id,
            key: payload.key,
            value: CanonicalValue::new(payload.value).map_err(|error| error.to_string())?,
        }])
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveSymbolPayload {
    symbol_id: String,
    position: jingwei_schematic::PointMm,
}

struct LogicalMoveSymbolHandler;

impl SemanticEditHandler for LogicalMoveSymbolHandler {
    fn command_kind(&self) -> &'static str {
        "schematic.move-symbol"
    }

    fn translate(
        &self,
        graph: &jingwei_jir::JirGraph,
        command: &SemanticEditCommand,
    ) -> Result<Vec<PatchOperation>, String> {
        let payload: MoveSymbolPayload = serde_json::from_value(command.payload.clone())
            .map_err(|error| format!("invalid schematic move-symbol payload: {error}"))?;
        let entity_id = EntityId::new(payload.symbol_id).map_err(|error| error.to_string())?;
        let entity = graph
            .entities
            .get(&entity_id)
            .ok_or_else(|| format!("logical symbol does not exist: {entity_id}"))?;
        if entity.kind.as_str() != "logical.symbol" {
            return Err(format!(
                "move-symbol target is not a logical.symbol: {entity_id}"
            ));
        }
        Ok(vec![PatchOperation::SetProperty {
            entity_id,
            key: "position".into(),
            value: CanonicalValue::new(
                serde_json::to_value(payload.position).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?,
        }])
    }
}

fn logical_tool_description(name: &str) -> &'static str {
    match name {
        "requirement_compile" => {
            "Validate a structured DesignIntent candidate and return directed questions"
        }
        "requirement_baseline" => "Return a confirmed intent candidate for kernel revision commit",
        "requirement_change_analyze" => {
            "Report invalidated logical decisions and validation scopes"
        }
        "risk_classify" => "Classify structured electrical and product risk facts",
        "part_search" => "Search authorized or offline normalized exact-MPN sources",
        "part_compare" => "Apply hard filters before auditable soft scoring",
        "part_select" => "Return an exact-MPN part selection candidate",
        "part_substitute_validate" => "Revalidate pinout, package and electrical equivalence",
        "datasheet_extract" => "Extract structured facts from an already controlled artifact",
        "library_resolve" => "Resolve exact MPN/package assets by policy precedence",
        "symbol_generate" => "Generate a project-private symbol from structured pin data",
        "footprint_generate" => "Generate a project-private footprint from structured package data",
        "library_verify" => "Validate provenance, pin-pad mapping and footprint geometry",
        "library_governance_evaluate" => {
            "Fail closed on stale, unlicensed, unapproved, revoked or hash-mismatched assets"
        }
        "library_diff" => "Compare versioned library asset semantics",
        "schematic_create" => "Create a native hierarchical logical graph candidate",
        "schematic_apply_change" => "Preview and return normalized semantic schematic operations",
        "schematic_connect" => "Connect or disconnect stable pin and net identities",
        "schematic_erc" => "Run native ERC with versioned coverage",
        "schematic_verify_custom" => "Run project-specific deterministic logical checks",
        "schematic_render" => "Return a schematic review display artifact candidate",
        "schematic_project" => "Return a typed KiCad 10.0.5 projection model and object map",
        _ => "Logical-domain operation",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn registration_contains_required_tools_without_approval_bypass() {
        let module = logical_module_registration();
        let names = module
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<BTreeSet<_>>();
        assert!(names.contains("requirement_compile"));
        assert!(names.contains("schematic_erc"));
        assert!(names.contains("schematic_project"));
        assert!(!names.contains("library_approve"));
        assert!(!names.contains("production_release"));
    }

    #[test]
    fn executable_registry_contains_all_logical_provider_kinds() {
        let registry = register_logical_module(ExtensionRegistryBuilder::new())
            .expect("register logical module")
            .build();
        assert_eq!(registry.tool_module_names(), vec!["jingwei-logical-domain"]);
        assert_eq!(registry.validator_names(), vec!["logical.namespaced-graph"]);
        assert_eq!(
            registry.view_provider_names(),
            vec!["logical.schematic-view"]
        );
        assert_eq!(registry.projection_names(), vec!["logical.kicad-schematic"]);
        assert_eq!(
            registry.edit_handler_names(),
            vec!["logical.set-property", "schematic.move-symbol"]
        );
        assert!(
            registry
                .entity_schema_kinds()
                .contains(&"logical.symbol".to_owned())
        );
    }
}
