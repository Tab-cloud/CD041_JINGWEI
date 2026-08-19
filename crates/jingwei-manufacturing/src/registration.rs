use async_trait::async_trait;
use jingwei_geometry::{BoundingBox, Point as BoardPoint, QuantizationPolicy};
use jingwei_jdp::{
    DisplayList, DisplayPrimitive, Effect, Hit, HitTestRequest, HitTestResult,
    JDP_PROTOCOL_VERSION, PatchOperation, Permission, Point as DisplayPoint, SemanticEditCommand,
    ToolDescriptor, ValidationFinding, ValidationOutput, ViewRequest,
};
use jingwei_jir::{EntityId, JirGraph, SchemaDescriptor};
use jingwei_kernel::{
    DomainToolModule, ExtensionRegistryBuilder, ProjectionArtifact, ProjectionBackend,
    ProjectionRequest, ProjectionResult, RegistryError, SemanticEditHandler, ToolContext,
    ToolRequest, ToolResult, ValidationContext, ValidationProvider, ViewProvider,
};
use jingwei_pcb::{
    Board, BoardDefinition, DisplayGeometry, LogicalNetlist, PCB_ROOT_ENTITY_ID, PcbRules,
};
use jingwei_placement::{PlacementRequest, place_baseline};
use jingwei_routing::{ReferenceRouter, RouteRequest, RouteStatus, RouterBackend};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, path::Path};
use tokio_util::sync::CancellationToken;

use crate::{
    ExportRequest, ManufacturerProfile, export_candidate, load_profile, validate_profile,
    verify_candidate,
};

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
    builder = builder.projection(PhysicalManufacturingProjectionBackend)?;
    builder = builder.edit_handler(PhysicalMoveFootprintHandler)?;
    for kind in [
        "physical.board",
        "physical.footprint",
        "physical.pad",
        "physical.track",
        "physical.via",
        "physical.zone",
        "physical.keepout",
        "physical.text",
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
                    "pcb_create_from_schematic"
                        | "pcb_place"
                        | "pcb_route_critical"
                        | "pcb_zone_fill"
                        | "pcb_drc"
                        | "design_equivalence_check"
                        | "manufacturing_profile_validate"
                        | "release_readiness"
                )
            })
            .collect()
    }

    async fn execute(
        &self,
        context: ToolContext,
        request: ToolRequest,
    ) -> Result<ToolResult, String> {
        let mut candidate_operations = Vec::new();
        let data = match request.name.as_str() {
            "pcb_create_from_schematic" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Input {
                    logical: LogicalNetlist,
                    definition: BoardDefinition,
                    rules: PcbRules,
                }
                if Board::from_jir_graph(&context.graph)
                    .map_err(|error| error.to_string())?
                    .is_some()
                {
                    return Err("the revision already contains a physical board".to_owned());
                }
                let input: Input = serde_json::from_value(request.input)
                    .map_err(|error| format!("invalid PCB creation input: {error}"))?;
                let board = Board::from_logical(&input.logical, input.definition, input.rules)
                    .map_err(|error| error.to_string())?;
                candidate_operations = replace_physical_graph(&context.graph, &board)?;
                json!({
                    "candidateOnly": true,
                    "candidateRevision": board.candidate_revision,
                    "boardHash": board.deterministic_hash(),
                    "footprintCount": board.footprints.len(),
                    "padCount": board.pads.len(),
                })
            }
            "pcb_place" => {
                let board = required_board(&context.graph)?;
                let placement_request: PlacementRequest = serde_json::from_value(request.input)
                    .map_err(|error| format!("invalid placement request: {error}"))?;
                let candidate = place_baseline(&board, &placement_request);
                if candidate.eligible() {
                    candidate_operations =
                        replace_physical_graph(&context.graph, &candidate.board)?;
                }
                serde_json::to_value(candidate).map_err(|error| error.to_string())?
            }
            "pcb_route_critical" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Input {
                    #[serde(default)]
                    included_net_ids: Vec<String>,
                    #[serde(default)]
                    excluded_net_ids: Vec<String>,
                    seed: u64,
                    time_budget_seconds: u64,
                    #[serde(default = "default_true")]
                    critical_only: bool,
                }
                let board = required_board(&context.graph)?;
                let input: Input = serde_json::from_value(request.input)
                    .map_err(|error| format!("invalid route request: {error}"))?;
                let candidate = ReferenceRouter
                    .route(
                        RouteRequest {
                            board,
                            included_net_ids: input.included_net_ids,
                            excluded_net_ids: input.excluded_net_ids,
                            seed: input.seed,
                            time_budget_seconds: input.time_budget_seconds,
                            critical_only: input.critical_only,
                        },
                        CancellationToken::new(),
                    )
                    .await;
                if candidate.status == RouteStatus::Completed && candidate.hard_findings.is_empty()
                {
                    candidate_operations =
                        replace_physical_graph(&context.graph, &candidate.board)?;
                }
                serde_json::to_value(candidate).map_err(|error| error.to_string())?
            }
            "pcb_zone_fill" => {
                let board = required_board(&context.graph)?;
                let candidate = board
                    .zone_fill_candidate()
                    .map_err(|error| error.to_string())?;
                candidate_operations = replace_physical_graph(&context.graph, &candidate)?;
                json!({
                    "candidateOnly": true,
                    "candidateRevision": candidate.candidate_revision,
                    "zoneCount": candidate.zones.len(),
                })
            }
            "pcb_drc" => serde_json::to_value(jingwei_drc::run_native_drc(&required_board(
                &context.graph,
            )?))
            .map_err(|error| error.to_string())?,
            "design_equivalence_check" => {
                let board = required_board(&context.graph)?;
                let projection = jingwei_kicad_pcb::project_board(&board);
                serde_json::to_value(jingwei_kicad_pcb::check_equivalence(&board, &projection))
                    .map_err(|error| error.to_string())?
            }
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
            candidate_operations,
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
        context: &ValidationContext,
    ) -> ValidationOutput {
        match Board::from_jir_graph(graph) {
            Ok(None) => ValidationOutput {
                valid: true,
                findings: Vec::new(),
                providers_run: vec![self.name().to_owned()],
            },
            Ok(Some(_board))
                if !context.scopes.iter().any(|scope| {
                    matches!(
                        scope.as_str(),
                        "physical" | "drc" | "pcb-native-drc" | "manufacturing"
                    )
                }) =>
            {
                ValidationOutput {
                    valid: true,
                    findings: Vec::new(),
                    providers_run: vec!["physical.jir-mirror".to_owned()],
                }
            }
            Ok(Some(board)) => {
                let report = jingwei_drc::run_native_drc(&board);
                ValidationOutput {
                    valid: report.error_count() == 0,
                    findings: report
                        .findings
                        .into_iter()
                        .map(|finding| ValidationFinding {
                            code: finding.rule_id,
                            severity: match finding.severity {
                                jingwei_drc::Severity::Error => "error",
                                jingwei_drc::Severity::Warning => "warning",
                                jingwei_drc::Severity::Info => "info",
                            }
                            .to_owned(),
                            summary: finding.title,
                            object_refs: finding.object_refs,
                            fingerprint: finding.id,
                        })
                        .collect(),
                    providers_run: vec![format!(
                        "jingwei-native-pcb-drc@{}",
                        jingwei_drc::NATIVE_DRC_VERSION
                    )],
                }
            }
            Err(error) => ValidationOutput {
                valid: false,
                findings: vec![ValidationFinding {
                    code: "PHYSICAL_JIR_INVALID".to_owned(),
                    severity: "error".to_owned(),
                    summary: error.to_string(),
                    object_refs: Vec::new(),
                    fingerprint: "physical-jir-invalid".to_owned(),
                }],
                providers_run: vec![self.name().to_owned()],
            },
        }
    }
}

struct PhysicalViewProvider;

#[async_trait]
impl ViewProvider for PhysicalViewProvider {
    fn name(&self) -> &'static str {
        "physical.pcb-view"
    }

    async fn view(
        &self,
        graph: &jingwei_jir::JirGraph,
        request: &ViewRequest,
    ) -> Result<DisplayList, String> {
        let Some(board) = Board::from_jir_graph(graph).map_err(|error| error.to_string())? else {
            return Ok(DisplayList {
                revision: request.revision.clone(),
                primitives: Vec::new(),
                artifact_refs: Vec::new(),
            });
        };
        let display = board.view(&jingwei_pcb::ViewRequest {
            viewport: BoundingBox::new(
                board_point(request.viewport.min),
                board_point(request.viewport.max),
            ),
            visible_layers: request.layers.clone(),
            lod: request.level_of_detail,
        });
        Ok(DisplayList {
            revision: request.revision.clone(),
            primitives: display
                .items
                .into_iter()
                .map(display_primitive)
                .collect::<Result<_, _>>()?,
            artifact_refs: Vec::new(),
        })
    }

    async fn hit_test(
        &self,
        graph: &jingwei_jir::JirGraph,
        request: &HitTestRequest,
    ) -> Result<HitTestResult, String> {
        let Some(board) = Board::from_jir_graph(graph).map_err(|error| error.to_string())? else {
            return Ok(HitTestResult { hits: Vec::new() });
        };
        Ok(HitTestResult {
            hits: board
                .hit_test(board_point(request.point), &QuantizationPolicy::default())
                .entity_ids
                .into_iter()
                .map(|entity_id| {
                    Ok(Hit {
                        entity_id: EntityId::new(entity_id).map_err(|error| error.to_string())?,
                        distance: 0.0,
                        primitive_kind: "physical".to_owned(),
                    })
                })
                .collect::<Result<_, String>>()?,
        })
    }
}

struct PhysicalProjectionBackend;

#[async_trait]
impl ProjectionBackend for PhysicalProjectionBackend {
    fn name(&self) -> &'static str {
        "physical.kicad-pcb"
    }

    async fn project(
        &self,
        graph: &jingwei_jir::JirGraph,
        request: &ProjectionRequest,
    ) -> Result<ProjectionResult, String> {
        let board = required_board(graph)?;
        let projection = jingwei_kicad_pcb::project_board(&board);
        let equivalence = jingwei_kicad_pcb::check_equivalence(&board, &projection);
        if !equivalence.equivalent {
            return Err(format!(
                "KiCad PCB projection failed semantic equivalence: {:?}",
                equivalence.notes
            ));
        }
        let object_map =
            serde_json::to_vec_pretty(&projection.object_map).map_err(|error| error.to_string())?;
        let projection_json =
            serde_json::to_vec_pretty(&projection).map_err(|error| error.to_string())?;
        Ok(ProjectionResult {
            artifacts: vec![
                ProjectionArtifact {
                    name: "board.kicad_pcb".to_owned(),
                    media_type: "application/x-kicad-pcb".to_owned(),
                    bytes: projection.content.as_bytes().to_vec(),
                },
                ProjectionArtifact {
                    name: "board.object-map.json".to_owned(),
                    media_type: "application/json".to_owned(),
                    bytes: object_map,
                },
                ProjectionArtifact {
                    name: "board.projection.json".to_owned(),
                    media_type: "application/json".to_owned(),
                    bytes: projection_json,
                },
            ],
            artifact_refs: Vec::new(),
            evidence_refs: board.evidence_refs.clone(),
            mapping: json!({
                "backend": request.backend,
                "revision": request.revision,
                "candidateRevision": board.candidate_revision,
                "boardHash": board.deterministic_hash(),
                "equivalence": equivalence,
                "coverage": projection.coverage,
            }),
        })
    }
}

struct PhysicalManufacturingProjectionBackend;

#[async_trait]
impl ProjectionBackend for PhysicalManufacturingProjectionBackend {
    fn name(&self) -> &'static str {
        "physical.manufacturing-package"
    }

    async fn project(
        &self,
        graph: &jingwei_jir::JirGraph,
        request: &ProjectionRequest,
    ) -> Result<ProjectionResult, String> {
        let board = required_board(graph)?;
        let profile = load_profile(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../profiles/manufacturers/jlcpcb-standard-v0.yaml"),
            1024 * 1024,
        )
        .map_err(|error| error.to_string())?;
        if board.rules.profile_id != profile.id {
            return Err(format!(
                "board profile {} does not match controlled export profile {}",
                board.rules.profile_id, profile.id
            ));
        }
        let native_drc = jingwei_drc::run_native_drc(&board);
        let temporary = tempfile::tempdir().map_err(|error| error.to_string())?;
        let variant = request
            .options
            .get("variant")
            .and_then(Value::as_str)
            .unwrap_or("default")
            .to_owned();
        let include_step = request
            .options
            .get("includeStep")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let candidate = export_candidate(
            &board,
            &profile,
            &native_drc,
            &ExportRequest {
                candidate_root: temporary.path().to_path_buf(),
                candidate_label: "jdp".to_owned(),
                variant,
                include_step,
            },
        )
        .map_err(|error| error.to_string())?;
        let mut verification = verify_candidate(&candidate.directory, &board, &profile);
        if !verification.valid_manufacturing_candidate {
            return Err(format!(
                "independent manufacturing verifier rejected the package: {:?}",
                verification.findings
            ));
        }
        "cas://manufacturing-candidate".clone_into(&mut verification.candidate_directory);
        let mut artifacts = candidate
            .manifest
            .outputs
            .iter()
            .map(|entry| {
                let bytes = fs::read(candidate.directory.join(&entry.path))
                    .map_err(|error| error.to_string())?;
                Ok(ProjectionArtifact {
                    name: entry.path.clone(),
                    media_type: manufacturing_media_type(&entry.path).to_owned(),
                    bytes,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        artifacts.push(ProjectionArtifact {
            name: "manifest.json".to_owned(),
            media_type: "application/json".to_owned(),
            bytes: fs::read(&candidate.manifest_path).map_err(|error| error.to_string())?,
        });
        artifacts.push(ProjectionArtifact {
            name: "manifest.sha256".to_owned(),
            media_type: "text/plain".to_owned(),
            bytes: fs::read(candidate.directory.join("manifest.sha256"))
                .map_err(|error| error.to_string())?,
        });
        artifacts.push(ProjectionArtifact {
            name: "reports/package-verification-attestation.json".to_owned(),
            media_type: "application/json".to_owned(),
            bytes: serde_json::to_vec_pretty(&json!({
                "schemaVersion": "jingwei-package-verification-attestation/0.1",
                "manifestSha256": candidate.manifest_sha256,
                "designRevision": candidate.manifest.design_revision,
                "verification": verification,
            }))
            .map_err(|error| error.to_string())?,
        });
        Ok(ProjectionResult {
            artifacts,
            artifact_refs: Vec::new(),
            evidence_refs: profile
                .source
                .iter()
                .map(|source| format!("manufacturer-source:{source}"))
                .collect(),
            mapping: json!({
                "backend": request.backend,
                "revision": request.revision,
                "candidateId": candidate.manifest.candidate_id,
                "manifestSha256": candidate.manifest_sha256,
                "maturity": candidate.manifest.maturity,
                "verification": verification,
                "openGates": candidate.manifest.open_gates,
            }),
        })
    }
}

fn manufacturing_media_type(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("json") => "application/json",
        Some("csv") => "text/csv",
        Some("svg") => "image/svg+xml",
        Some("kicad_pcb") => "application/x-kicad-pcb",
        Some("step") => "model/step",
        _ => "application/octet-stream",
    }
}

fn required_board(graph: &JirGraph) -> Result<Board, String> {
    Board::from_jir_graph(graph)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "the revision has no physical board candidate".to_owned())
}

fn replace_physical_graph(graph: &JirGraph, board: &Board) -> Result<Vec<PatchOperation>, String> {
    let physical_ids = graph
        .entities
        .values()
        .filter(|entity| entity.kind.as_str().starts_with("physical."))
        .map(|entity| entity.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let mut operations = graph
        .edges
        .values()
        .filter(|edge| {
            edge.kind.starts_with("physical.")
                || physical_ids.contains(&edge.from)
                || physical_ids.contains(&edge.to)
        })
        .map(|edge| PatchOperation::RemoveEdge {
            edge_id: edge.id.clone(),
        })
        .collect::<Vec<_>>();
    operations.extend(
        graph
            .entities
            .values()
            .filter(|entity| {
                entity.kind.as_str().starts_with("physical.")
                    && entity.id.as_str() != PCB_ROOT_ENTITY_ID
            })
            .map(|entity| PatchOperation::RemoveEntity {
                entity_id: entity.id.clone(),
            }),
    );
    if let Some(root) = graph
        .entities
        .values()
        .find(|entity| entity.id.as_str() == PCB_ROOT_ENTITY_ID)
    {
        operations.push(PatchOperation::RemoveEntity {
            entity_id: root.id.clone(),
        });
    }
    let physical = board.to_jir_graph().map_err(|error| error.to_string())?;
    operations.extend(
        physical
            .entities
            .into_values()
            .map(|entity| PatchOperation::AddEntity { entity }),
    );
    operations.extend(
        physical
            .edges
            .into_values()
            .map(|edge| PatchOperation::AddEdge { edge }),
    );
    Ok(operations)
}

fn display_primitive(item: jingwei_pcb::DisplayItem) -> Result<DisplayPrimitive, String> {
    let entity_id = EntityId::new(item.entity_id).map_err(|error| error.to_string())?;
    Ok(match item.geometry {
        DisplayGeometry::Bounds { bounds } => DisplayPrimitive::Polygon {
            points: vec![
                display_point(bounds.min),
                DisplayPoint {
                    x: millimetres(bounds.max.x),
                    y: millimetres(bounds.min.y),
                },
                display_point(bounds.max),
                DisplayPoint {
                    x: millimetres(bounds.min.x),
                    y: millimetres(bounds.max.y),
                },
            ],
            entity_id,
        },
        DisplayGeometry::Polyline { path, .. } => DisplayPrimitive::Polyline {
            points: path.points.into_iter().map(display_point).collect(),
            entity_id,
        },
        DisplayGeometry::Polygon { boundary } => DisplayPrimitive::Polygon {
            points: boundary.vertices.into_iter().map(display_point).collect(),
            entity_id,
        },
        DisplayGeometry::Circle { center, .. } => DisplayPrimitive::Marker {
            position: display_point(center),
            marker: item.entity_kind,
            entity_id,
        },
        DisplayGeometry::Text { position, text } => DisplayPrimitive::Text {
            position: display_point(position),
            text,
            entity_id,
        },
    })
}

#[allow(clippy::cast_possible_truncation)]
fn board_point(point: DisplayPoint) -> BoardPoint {
    BoardPoint::from_micrometres(
        (point.x * 1_000.0).round() as i64,
        (point.y * 1_000.0).round() as i64,
    )
}

fn display_point(point: BoardPoint) -> DisplayPoint {
    DisplayPoint {
        x: millimetres(point.x),
        y: millimetres(point.y),
    }
}

#[allow(clippy::cast_precision_loss)]
fn millimetres(length: jingwei_geometry::Length) -> f64 {
    length.as_micrometres() as f64 / 1_000.0
}

const fn default_true() -> bool {
    true
}

struct PhysicalMoveFootprintHandler;

impl SemanticEditHandler for PhysicalMoveFootprintHandler {
    fn command_kind(&self) -> &'static str {
        "pcb.move-footprint"
    }

    fn translate(
        &self,
        graph: &jingwei_jir::JirGraph,
        command: &SemanticEditCommand,
    ) -> Result<Vec<PatchOperation>, String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Payload {
            footprint_id: String,
            position: DisplayPoint,
        }
        let payload: Payload = serde_json::from_value(command.payload.clone())
            .map_err(|error| format!("invalid PCB move-footprint payload: {error}"))?;
        let board = required_board(graph)?;
        let candidate = board
            .commit_edit_candidate(
                &[jingwei_pcb::SemanticEditCommand::MoveFootprint {
                    entity_id: payload.footprint_id,
                    position: board_point(payload.position),
                }],
                &QuantizationPolicy::default(),
            )
            .map_err(|error| error.to_string())?;
        replace_physical_graph(graph, &candidate.board)
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
        requires_base_revision: matches!(effect, Effect::RevisionWrite | Effect::DraftWrite),
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
        assert_eq!(
            registry.projection_names(),
            vec!["physical.kicad-pcb", "physical.manufacturing-package"]
        );
        assert_eq!(registry.edit_handler_names(), vec!["pcb.move-footprint"]);
        assert!(
            registry
                .entity_schema_kinds()
                .contains(&"physical.board".to_owned())
        );
    }
}
