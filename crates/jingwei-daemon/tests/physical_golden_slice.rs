use std::{env, fs, path::Path, sync::Arc};

use jingwei_daemon::{Daemon, InMemoryClient, production_registry};
use jingwei_geometry::{Length, Point as BoardPoint, Polygon};
use jingwei_jdp::{
    ArtifactGetInput, ArtifactGetOutput, DesignHitTestInput, DesignProjectInput,
    DesignProjectOutput, DesignViewInput, DisplayList, DisplayPrimitive, EditCommandGroup,
    EditCommitOutput, EditPreviewOutput, EditSessionInfo, EditSessionOpenInput, HitTestRequest,
    JDP_PROTOCOL_VERSION, JdpClient, JdpRequest, JdpResponse, PatchApplyInput, PatchInput,
    PatchOutput, Point, ProjectCreateInput, ProjectInfo, ProposalDecisionInput,
    ProposalDecisionOutput, Rect, SemanticEditCommand, ValidateInput, ValidationOutput,
    ViewRequest, operation,
};
use jingwei_jir::{
    ArtifactId, ExecutionId, IdempotencyKey, ProjectId, ProposalAction, RequestId, RevisionId,
    TaskId, WorkspaceId,
};
use jingwei_kernel::ToolResult;
use jingwei_pcb::{
    BoardDefinition, CopperLayer, LayerKind, LogicalNetlist, MechanicalFeature, Stackup,
};
use serde_json::{Value, json};
use tempfile::tempdir;

#[tokio::test]
async fn golden_pcb_is_placed_routed_rendered_edited_and_projected() {
    let directory = tempdir().unwrap();
    let workspace = WorkspaceId::new("ws-physical-golden").unwrap();
    let project = ProjectId::new("project-physical-golden").unwrap();
    let daemon = Arc::new(
        Daemon::create(
            directory.path(),
            workspace.clone(),
            production_registry().unwrap(),
        )
        .unwrap(),
    );
    let client = InMemoryClient::connect(daemon, "physical-golden-test").unwrap();
    let profile = jingwei_manufacturing::load_profile(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../profiles/manufacturers/jlcpcb-standard-v0.yaml"),
        1024 * 1024,
    )
    .unwrap();
    let created: ProjectInfo = success(
        client
            .send(request(
                &workspace,
                &project,
                operation::PROJECT_CREATE,
                "create",
                None,
                None,
                json!(ProjectCreateInput {
                    name: "Physical golden".to_owned(),
                }),
            ))
            .await
            .unwrap(),
    );

    let logical: LogicalNetlist = serde_json::from_str(include_str!(
        "../../../testdata/physical/logical-netlist-v0/golden-board.json"
    ))
    .unwrap();
    let create_candidate: ToolResult = success(
        client
            .send(request(
                &workspace,
                &project,
                "pcb_create_from_schematic",
                "pcb-create",
                Some(created.current_revision.clone()),
                Some(task("pcb-create")),
                json!({
                    "logical": logical,
                    "definition": board_definition(),
                    "rules": profile.pcb_rules(),
                }),
            ))
            .await
            .unwrap(),
    );
    assert!(!create_candidate.candidate_operations.is_empty());
    let created_board = apply_and_accept(
        &client,
        &workspace,
        &project,
        created.current_revision,
        "pcb-create",
        create_candidate,
        vec!["graph.integrity".to_owned()],
    )
    .await;

    let placement: ToolResult = success(
        client
            .send(request(
                &workspace,
                &project,
                "pcb_place",
                "pcb-place",
                Some(created_board.clone()),
                Some(task("pcb-place")),
                json!({
                    "seed": 0x4a57_3034_u64,
                    "selectedFootprintIds": [],
                    "region": null,
                    "gridStep": 5000,
                }),
            ))
            .await
            .unwrap(),
    );
    assert_eq!(placement.data["hardViolations"], json!([]));
    let placed = apply_and_accept(
        &client,
        &workspace,
        &project,
        created_board,
        "pcb-place",
        placement,
        vec!["graph.integrity".to_owned()],
    )
    .await;

    let route: ToolResult = success(
        client
            .send(request(
                &workspace,
                &project,
                "pcb_route_critical",
                "pcb-route",
                Some(placed.clone()),
                Some(task("pcb-route")),
                json!({
                    "includedNetIds": [],
                    "excludedNetIds": [],
                    "seed": 0x4a57_3034_u64,
                    "timeBudgetSeconds": 10,
                    "criticalOnly": false,
                }),
            ))
            .await
            .unwrap(),
    );
    assert_eq!(route.data["status"], "completed");
    let routed = apply_and_accept(
        &client,
        &workspace,
        &project,
        placed,
        "pcb-route",
        route,
        vec!["graph.integrity".to_owned()],
    )
    .await;

    let validation: ValidationOutput = success(
        client
            .send(request(
                &workspace,
                &project,
                operation::DESIGN_VALIDATE,
                "pcb-validate",
                None,
                None,
                json!(ValidateInput {
                    revision: Some(routed.clone()),
                    scopes: vec!["pcb-native-drc".to_owned()],
                }),
            ))
            .await
            .unwrap(),
    );
    assert!(validation.valid, "{:?}", validation.findings);
    assert!(
        validation
            .providers_run
            .iter()
            .any(|provider| provider.starts_with("jingwei-native-pcb-drc@"))
    );

    let display = view(&client, &workspace, &project, &routed).await;
    assert!(display.primitives.len() >= 8);
    let (footprint_id, point) = display
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            DisplayPrimitive::Polygon { points, entity_id }
                if entity_id.as_str() == "pcb-footprint:component:R1" =>
            {
                Some((entity_id.clone(), points[0]))
            }
            _ => None,
        })
        .unwrap();
    let hit: jingwei_jdp::HitTestResult = success(
        client
            .send(request(
                &workspace,
                &project,
                operation::DESIGN_HIT_TEST,
                "pcb-hit",
                None,
                None,
                json!(DesignHitTestInput {
                    provider: "physical.pcb-view".to_owned(),
                    request: HitTestRequest {
                        revision: routed.clone(),
                        point,
                        tolerance: 1.0,
                        layers: Vec::new(),
                    },
                }),
            ))
            .await
            .unwrap(),
    );
    assert!(!hit.hits.is_empty());

    let projected: DesignProjectOutput = success(
        client
            .send(request(
                &workspace,
                &project,
                operation::DESIGN_PROJECT,
                "pcb-project",
                None,
                None,
                json!(DesignProjectInput {
                    provider: "physical.kicad-pcb".to_owned(),
                    revision: routed.clone(),
                    options: json!({"target": "10.0.5"}),
                }),
            ))
            .await
            .unwrap(),
    );
    assert_eq!(projected.artifact_refs.len(), 3);
    assert_eq!(projected.mapping["equivalence"]["equivalent"], true);
    if let Some(output) = env::var_os("JINGWEI_PHYSICAL_GOLDEN_OUT") {
        materialize_projection(
            &client,
            &workspace,
            &project,
            &projected,
            Path::new(&output),
        )
        .await;
    }
    let pcb_bytes = artifact(&client, &workspace, &project, &projected, "board.kicad_pcb").await;
    let pcb_text = String::from_utf8(pcb_bytes).unwrap();
    assert_eq!(
        jingwei_kicad_pcb::round_trip_projection(&pcb_text, 16 * 1024 * 1024).unwrap(),
        pcb_text
    );
    let manufacturing: DesignProjectOutput = success(
        client
            .send(request(
                &workspace,
                &project,
                operation::DESIGN_PROJECT,
                "manufacturing-project",
                None,
                None,
                json!(DesignProjectInput {
                    provider: "physical.manufacturing-package".to_owned(),
                    revision: routed.clone(),
                    options: json!({"variant": "default", "includeStep": true}),
                }),
            ))
            .await
            .unwrap(),
    );
    assert_eq!(
        manufacturing.mapping["verification"]["validManufacturingCandidate"],
        true
    );
    assert!(manufacturing.artifact_refs.len() >= 20);
    let manifest_bytes = artifact(
        &client,
        &workspace,
        &project,
        &manufacturing,
        "manifest.json",
    )
    .await;
    let manifest: jingwei_manufacturing::Manifest =
        serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest.maturity, "L3-manufacturing-candidate-at-most");
    assert!(!manifest.open_gates.is_empty());
    if let Some(output) = env::var_os("JINGWEI_MANUFACTURING_GOLDEN_OUT") {
        materialize_projection(
            &client,
            &workspace,
            &project,
            &manufacturing,
            Path::new(&output),
        )
        .await;
    }

    let edit_session: EditSessionInfo = success(
        client
            .send(request(
                &workspace,
                &project,
                operation::EDIT_SESSION_OPEN,
                "pcb-edit-open",
                None,
                None,
                json!(EditSessionOpenInput {
                    base_revision: routed,
                    lease_seconds: 1_800,
                }),
            ))
            .await
            .unwrap(),
    );
    let group = EditCommandGroup {
        edit_session_id: edit_session.edit_session_id,
        base_draft_revision: edit_session.draft_head_revision,
        commands: vec![SemanticEditCommand {
            kind: "pcb.move-footprint".to_owned(),
            payload: json!({"footprintId": footprint_id, "position": {"x": 30.0, "y": 20.0}}),
        }],
        expected_postconditions: vec!["graph.integrity".to_owned()],
        preview_hash: None,
    };
    let preview: EditPreviewOutput = success(
        client
            .send(request(
                &workspace,
                &project,
                operation::EDIT_COMMAND_PREVIEW,
                "pcb-edit-preview",
                None,
                None,
                json!(group),
            ))
            .await
            .unwrap(),
    );
    assert!(preview.validation.valid);
    let commit: EditCommitOutput = success(
        client
            .send(request(
                &workspace,
                &project,
                operation::EDIT_COMMAND_COMMIT,
                "pcb-edit-commit",
                None,
                None,
                json!(EditCommandGroup {
                    preview_hash: Some(preview.preview_hash),
                    ..group
                }),
            ))
            .await
            .unwrap(),
    );
    assert_ne!(commit.draft_revision, edit_session.base_project_revision);
}

async fn materialize_projection(
    client: &InMemoryClient,
    workspace: &WorkspaceId,
    project: &ProjectId,
    projected: &DesignProjectOutput,
    output: &Path,
) {
    fs::create_dir_all(output).unwrap();
    for item in projected.mapping["artifacts"].as_array().unwrap() {
        let name = item["name"].as_str().unwrap();
        let bytes = artifact(client, workspace, project, projected, name).await;
        let path = output.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }
    if projected.mapping["backend"] == "physical.kicad-pcb" {
        fs::write(output.join("board.kicad_pro"), b"{}\n").unwrap();
    }
}

async fn apply_and_accept(
    client: &InMemoryClient,
    workspace: &WorkspaceId,
    project: &ProjectId,
    base: RevisionId,
    key: &str,
    candidate: ToolResult,
    expected_postconditions: Vec<String>,
) -> RevisionId {
    let applied: PatchOutput = success(
        client
            .send(request(
                workspace,
                project,
                operation::DESIGN_PATCH_APPLY,
                &format!("{key}-apply"),
                Some(base),
                None,
                json!(PatchApplyInput {
                    proposal_id: None,
                    patch: PatchInput {
                        operations: candidate.candidate_operations,
                        preconditions: Vec::new(),
                        expected_postconditions,
                    },
                }),
            ))
            .await
            .unwrap(),
    );
    let accepted: ProposalDecisionOutput = success(
        client
            .send(request(
                workspace,
                project,
                operation::CHANGE_PROPOSAL_DECIDE,
                &format!("{key}-accept"),
                None,
                None,
                json!(ProposalDecisionInput {
                    proposal_id: applied.proposal_id,
                    action: ProposalAction::Accept,
                    reason: Some(format!("accept {key}")),
                }),
            ))
            .await
            .unwrap(),
    );
    accepted.project_head
}

async fn view(
    client: &InMemoryClient,
    workspace: &WorkspaceId,
    project: &ProjectId,
    revision: &RevisionId,
) -> DisplayList {
    success(
        client
            .send(request(
                workspace,
                project,
                operation::DESIGN_VIEW,
                "pcb-view",
                None,
                None,
                json!(DesignViewInput {
                    provider: "physical.pcb-view".to_owned(),
                    request: ViewRequest {
                        revision: revision.clone(),
                        viewport: Rect {
                            min: Point { x: 0.0, y: 0.0 },
                            max: Point { x: 60.0, y: 40.0 },
                        },
                        layers: Vec::new(),
                        level_of_detail: 2,
                        selection: Vec::new(),
                    },
                }),
            ))
            .await
            .unwrap(),
    )
}

async fn artifact(
    client: &InMemoryClient,
    workspace: &WorkspaceId,
    project: &ProjectId,
    projected: &DesignProjectOutput,
    name: &str,
) -> Vec<u8> {
    let artifact_id: ArtifactId = projected.mapping["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|artifact| artifact["name"] == name)
        .unwrap()["artifactId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let output: ArtifactGetOutput = success(
        client
            .send(request(
                workspace,
                project,
                operation::ARTIFACT_GET,
                &format!("pcb-artifact-{}", safe_key(name)),
                None,
                None,
                json!(ArtifactGetInput { artifact_id }),
            ))
            .await
            .unwrap(),
    );
    hex::decode(output.bytes_hex).unwrap()
}

fn safe_key(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn board_definition() -> BoardDefinition {
    BoardDefinition {
        outline: Polygon::rectangle(
            BoardPoint::from_millimetres(0, 0),
            BoardPoint::from_millimetres(60, 40),
        ),
        cutouts: Vec::new(),
        mounting_features: [("H1", 3, 3), ("H2", 57, 3), ("H3", 3, 37), ("H4", 57, 37)]
            .into_iter()
            .map(|(id, x, y)| MechanicalFeature {
                id: format!("mechanical:{id}"),
                center: BoardPoint::from_millimetres(x, y),
                drill: Length::from_millimetres(3),
                slot_length: Length::ZERO,
                plated: false,
                locked: true,
            })
            .collect(),
        stackup: Stackup {
            board_thickness: Length::from_micrometres(1_600),
            layers: [
                ("F.Cu", LayerKind::Copper, 35),
                ("B.Cu", LayerKind::Copper, 35),
                ("F.Mask", LayerKind::SolderMask, 0),
                ("B.Mask", LayerKind::SolderMask, 0),
                ("F.SilkS", LayerKind::Silkscreen, 0),
                ("B.SilkS", LayerKind::Silkscreen, 0),
                ("Edge.Cuts", LayerKind::EdgeCuts, 0),
            ]
            .into_iter()
            .map(|(name, kind, copper)| CopperLayer {
                id: format!("layer:{name}"),
                name: name.to_owned(),
                kind,
                copper_thickness: Length::from_micrometres(copper),
            })
            .collect(),
            surface_finish: "HASL lead-free".to_owned(),
        },
        regions: Vec::new(),
        origin: BoardPoint::from_millimetres(0, 0),
    }
}

fn task(key: &str) -> TaskId {
    TaskId::new(format!("task-{key}")).unwrap()
}

fn success<T>(response: JdpResponse<T>) -> T {
    match response {
        JdpResponse::Success(value) => value.data,
        JdpResponse::Failure(error) => panic!("JDP failure: {:?}", error.error),
    }
}

fn request(
    workspace: &WorkspaceId,
    project: &ProjectId,
    operation_name: &str,
    key: &str,
    base_revision: Option<RevisionId>,
    task_id: Option<TaskId>,
    input: Value,
) -> JdpRequest<Value> {
    JdpRequest {
        protocol_version: JDP_PROTOCOL_VERSION.to_owned(),
        request_id: RequestId::new(format!("request-{key}")).unwrap(),
        idempotency_key: IdempotencyKey::new(format!("idem-{key}")).unwrap(),
        workspace_id: workspace.clone(),
        project_id: project.clone(),
        operation: operation_name.to_owned(),
        execution_id: Some(ExecutionId::new(format!("execution-{key}")).unwrap()),
        task_id,
        base_revision,
        deadline_ms: Some(30_000),
        dry_run: false,
        trace_context: None,
        input,
    }
}
