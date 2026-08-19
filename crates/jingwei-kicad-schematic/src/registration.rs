use async_trait::async_trait;
use jingwei_components::{DesignIntent, classify_risk};
use jingwei_jdp::{
    DisplayList, Effect, HitTestRequest, HitTestResult, PatchOperation, Permission,
    SemanticEditCommand, ToolDescriptor, ValidationOutput, ViewRequest,
};
use jingwei_jir::{CanonicalValue, EntityId, SchemaDescriptor};
use jingwei_kernel::{
    DomainToolModule, ExtensionRegistryBuilder, ProjectionBackend, ProjectionRequest,
    ProjectionResult, RegistryError, SemanticEditHandler, ToolContext, ToolRequest, ToolResult,
    ValidationContext, ValidationProvider, ViewProvider,
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
                    "requirement_compile" | "risk_classify"
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
        let invalid = graph
            .entities
            .values()
            .filter(|entity| entity.kind.as_str().starts_with("logical."))
            .any(|entity| entity.schema_version.trim().is_empty());
        ValidationOutput {
            valid: !invalid,
            findings: Vec::new(),
            providers_run: vec![self.name().to_owned()],
        }
    }
}

struct LogicalViewProvider;

#[async_trait]
impl ViewProvider for LogicalViewProvider {
    fn name(&self) -> &'static str {
        "logical.schematic-view"
    }

    async fn view(&self, request: &ViewRequest) -> Result<DisplayList, String> {
        Ok(DisplayList {
            revision: request.revision.clone(),
            primitives: Vec::new(),
            artifact_refs: Vec::new(),
        })
    }

    async fn hit_test(&self, _request: &HitTestRequest) -> Result<HitTestResult, String> {
        Ok(HitTestResult { hits: Vec::new() })
    }
}

struct LogicalProjectionBackend;

#[async_trait]
impl ProjectionBackend for LogicalProjectionBackend {
    fn name(&self) -> &'static str {
        "logical.kicad-schematic"
    }

    async fn project(&self, request: &ProjectionRequest) -> Result<ProjectionResult, String> {
        Ok(ProjectionResult {
            artifact_refs: Vec::new(),
            evidence_refs: Vec::new(),
            mapping: json!({
                "backend": request.backend,
                "revision": request.revision,
                "state": "registered; artifact materialization requires a domain tool input",
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
        assert_eq!(registry.edit_handler_names(), vec!["logical.set-property"]);
        assert!(
            registry
                .entity_schema_kinds()
                .contains(&"logical.symbol".to_owned())
        );
    }
}
