use async_trait::async_trait;
use jingwei_jdp::{
    DisplayList, Effect, HitTestRequest, HitTestResult, JDP_PROTOCOL_VERSION, PatchOperation,
    Permission, SemanticEditCommand, ToolDescriptor, ValidationOutput, ViewRequest,
};
use jingwei_jir::{CanonicalValue, EntityId, SchemaDescriptor};
use jingwei_kernel::{
    DomainToolModule, ExtensionRegistryBuilder, ProjectionBackend, ProjectionRequest,
    ProjectionResult, RegistryError, SemanticEditHandler, ToolContext, ToolRequest, ToolResult,
    ValidationContext, ValidationProvider, ViewProvider,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{ManufacturerProfile, validate_profile};

/// Task-02-owned descriptor type used by the physical module.
pub type PhysicalToolDescriptor = ToolDescriptor;

/// Physical module registration metadata for the task-02 extension registry.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalModuleRegistration {
    /// Module ID.
    pub module_id: String,
    /// Domain implementation version.
    pub version: String,
    /// Tool descriptors.
    pub tools: Vec<PhysicalToolDescriptor>,
    /// Formal integration requirement.
    pub integration_note: String,
}

/// Registers the task-04 tool surface without claiming a daemon endpoint.
pub fn physical_module_registration() -> PhysicalModuleRegistration {
    let write = |name: &str| {
        descriptor(
            name,
            Permission::CandidateWrite,
            Effect::RevisionWrite,
            true,
        )
    };
    let read = |name: &str| descriptor(name, Permission::Read, Effect::ReadOnly, false);
    let external =
        |name: &str| descriptor(name, Permission::ExternalProcess, Effect::ReadOnly, true);
    PhysicalModuleRegistration {
        module_id: "jingwei-physical-manufacturing".to_owned(),
        version: "0.1.0".to_owned(),
        tools: vec![
            write("pcb_create_from_schematic"),
            write("pcb_profile_apply"),
            write("pcb_mechanical_import"),
            write("pcb_place"),
            write("pcb_route_critical"),
            write("pcb_autoroute"),
            write("pcb_zone_fill"),
            read("pcb_drc"),
            read("pcb_verify_custom"),
            read("pcb_render"),
            external("pcb_project"),
            read("design_equivalence_check"),
            read("manufacturing_profile_validate"),
            descriptor(
                "manufacturing_export",
                Permission::ReleaseCandidate,
                Effect::RevisionWrite,
                true,
            ),
            read("manufacturing_package_verify"),
            read("release_readiness"),
        ],
        integration_note: "Adapt these handlers to task-02 DomainToolModule/ToolDescriptor; only the kernel transaction service may commit a candidate revision.".to_owned(),
    }
}

/// Registers executable physical/manufacturing adapters into the Task 02 registry.
pub fn register_physical_module(
    mut builder: ExtensionRegistryBuilder,
) -> Result<ExtensionRegistryBuilder, RegistryError> {
    builder = builder.tool_module(PhysicalToolModule)?;
    builder = builder.validator(PhysicalGraphValidator)?;
    builder = builder.view_provider(PhysicalViewProvider)?;
    builder = builder.projection(PhysicalProjectionBackend)?;
    builder = builder.edit_handler(PhysicalSetPropertyHandler)?;
    for kind in [
        "physical.board",
        "physical.footprint",
        "physical.pad",
        "physical.track",
        "physical.via",
        "physical.zone",
        "manufacturing.candidate",
    ] {
        builder = builder.entity_schema(open_schema(kind))?;
    }
    for kind in ["physical.contains", "physical.connects"] {
        builder = builder.edge_schema(open_schema(kind))?;
    }
    Ok(builder)
}

fn open_schema(kind: &str) -> SchemaDescriptor {
    SchemaDescriptor {
        kind: kind.to_owned(),
        schema_version: "physical-domain/0.1".to_owned(),
        json_schema: json!({"type": "object", "additionalProperties": true}),
    }
}

struct PhysicalToolModule;

#[async_trait]
impl DomainToolModule for PhysicalToolModule {
    fn module_name(&self) -> &'static str {
        "jingwei-physical-manufacturing"
    }

    fn descriptors(&self) -> Vec<ToolDescriptor> {
        physical_module_registration()
            .tools
            .into_iter()
            .filter(|descriptor| {
                matches!(
                    descriptor.name.as_str(),
                    "manufacturing_profile_validate" | "release_readiness"
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
            "manufacturing_profile_validate" => {
                let profile: ManufacturerProfile = serde_json::from_value(request.input)
                    .map_err(|error| format!("invalid manufacturer profile: {error}"))?;
                serde_json::to_value(validate_profile(&profile))
                    .map_err(|error| error.to_string())?
            }
            "release_readiness" => {
                let candidate_id = request
                    .input
                    .get("candidateId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "candidateId is required".to_owned())?;
                serde_json::to_value(release_readiness(candidate_id))
                    .map_err(|error| error.to_string())?
            }
            _ => {
                return Err(format!(
                    "physical operation is not executable: {}",
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

struct PhysicalGraphValidator;

#[async_trait]
impl ValidationProvider for PhysicalGraphValidator {
    fn name(&self) -> &'static str {
        "physical.namespaced-graph"
    }

    async fn validate(
        &self,
        graph: &jingwei_jir::JirGraph,
        _context: &ValidationContext,
    ) -> ValidationOutput {
        let invalid = graph
            .entities
            .values()
            .filter(|entity| entity.kind.as_str().starts_with("physical."))
            .any(|entity| entity.schema_version.trim().is_empty());
        ValidationOutput {
            valid: !invalid,
            findings: Vec::new(),
            providers_run: vec![self.name().to_owned()],
        }
    }
}

struct PhysicalViewProvider;

#[async_trait]
impl ViewProvider for PhysicalViewProvider {
    fn name(&self) -> &'static str {
        "physical.pcb-view"
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

struct PhysicalProjectionBackend;

#[async_trait]
impl ProjectionBackend for PhysicalProjectionBackend {
    fn name(&self) -> &'static str {
        "physical.kicad-pcb"
    }

    async fn project(&self, request: &ProjectionRequest) -> Result<ProjectionResult, String> {
        Ok(ProjectionResult {
            artifact_refs: Vec::new(),
            evidence_refs: Vec::new(),
            mapping: json!({
                "backend": request.backend,
                "revision": request.revision,
                "state": "registered; candidate export remains an explicit domain tool",
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

struct PhysicalSetPropertyHandler;

impl SemanticEditHandler for PhysicalSetPropertyHandler {
    fn command_kind(&self) -> &'static str {
        "physical.set-property"
    }

    fn translate(
        &self,
        graph: &jingwei_jir::JirGraph,
        command: &SemanticEditCommand,
    ) -> Result<Vec<PatchOperation>, String> {
        let payload: SetPropertyPayload = serde_json::from_value(command.payload.clone())
            .map_err(|error| format!("invalid physical set-property payload: {error}"))?;
        let entity_id = EntityId::new(payload.entity_id).map_err(|error| error.to_string())?;
        if !graph.entities.contains_key(&entity_id) {
            return Err(format!("physical entity does not exist: {entity_id}"));
        }
        Ok(vec![PatchOperation::SetProperty {
            entity_id,
            key: payload.key,
            value: CanonicalValue::new(payload.value).map_err(|error| error.to_string())?,
        }])
    }
}

fn descriptor(
    name: &str,
    permission: Permission,
    effect: Effect,
    cancellable: bool,
) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_owned(),
        description: format!("Jingwei physical/manufacturing domain operation: {name}"),
        input_schema: json!({"type": "object", "additionalProperties": true}),
        output_schema: json!({"type": "object", "additionalProperties": true}),
        permission,
        effect,
        resource_claim_template: vec![
            "workspace:{workspaceId}".to_owned(),
            "project:{projectId}".to_owned(),
        ],
        idempotent: true,
        cancellable,
        default_timeout_ms: if cancellable { 300_000 } else { 30_000 },
        protocol_version: JDP_PROTOCOL_VERSION.to_owned(),
        schema_version: "0.1".to_owned(),
    }
}

/// Release-readiness report that cannot create a Production Release.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseReadiness {
    /// Manufacturing candidate ID.
    pub candidate_id: String,
    /// Highest maturity this task can establish.
    pub maturity: String,
    /// Whether Production Release is available from this module.
    pub production_release_available: bool,
    /// Mandatory missing gates.
    pub blocking_gaps: Vec<String>,
}

/// Reports the required external gates without fabricating approvals or test records.
pub fn release_readiness(candidate_id: &str) -> ReleaseReadiness {
    ReleaseReadiness {
        candidate_id: candidate_id.to_owned(),
        maturity: "L3-manufacturing-candidate-at-most".to_owned(),
        production_release_available: false,
        blocking_gaps: vec![
            "G6 manufacturer DFM feedback and disposition".to_owned(),
            "G7 physical prototype test records and measurements".to_owned(),
            "current-revision named hardware/PCB/manufacturing/test/supply approvals".to_owned(),
            "organization-controlled signed Production Release manifest".to_owned(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_has_no_production_release_tool() {
        let module = physical_module_registration();
        assert!(
            module
                .tools
                .iter()
                .all(|tool| tool.name != "production_release")
        );
        let readiness = release_readiness("candidate-1");
        assert!(!readiness.production_release_available);
        assert!(readiness.blocking_gaps.len() >= 3);
    }

    #[test]
    fn executable_registry_contains_all_physical_provider_kinds() {
        let registry = register_physical_module(ExtensionRegistryBuilder::new())
            .expect("register physical module")
            .build();
        assert_eq!(
            registry.tool_module_names(),
            vec!["jingwei-physical-manufacturing"]
        );
        assert_eq!(
            registry.validator_names(),
            vec!["physical.namespaced-graph"]
        );
        assert_eq!(registry.view_provider_names(), vec!["physical.pcb-view"]);
        assert_eq!(registry.projection_names(), vec!["physical.kicad-pcb"]);
        assert_eq!(registry.edit_handler_names(), vec!["physical.set-property"]);
        assert!(
            registry
                .entity_schema_kinds()
                .contains(&"physical.board".to_owned())
        );
    }
}
