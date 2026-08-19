use jingwei_jdp::{Effect, JDP_PROTOCOL_VERSION, Permission, ToolDescriptor};
use serde::{Deserialize, Serialize};
use serde_json::json;

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
pub fn register_physical_module() -> PhysicalModuleRegistration {
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
        let module = register_physical_module();
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
}
