use std::collections::{BTreeMap, BTreeSet};
use std::{env, fs, path::Path, sync::Arc};

use jingwei_daemon::{Daemon, InMemoryClient, production_registry};
use jingwei_jdp::{
    ArtifactGetInput, ArtifactGetOutput, DesignHitTestInput, DesignProjectInput,
    DesignProjectOutput, DesignViewInput, DisplayList, DisplayPrimitive, EditCommandGroup,
    EditCommitOutput, EditPreviewOutput, EditSessionInfo, EditSessionOpenInput, EditSessionRef,
    HitTestRequest, JDP_PROTOCOL_VERSION, JdpClient, JdpRequest, JdpResponse, PatchApplyInput,
    PatchInput, PatchOutput, Point, ProjectCreateInput, ProjectInfo, ProposalDecisionInput,
    ProposalDecisionOutput, Rect, SemanticEditCommand, ValidateInput, ValidationOutput,
    ViewRequest, operation,
};
use jingwei_jir::{
    ArtifactId, EditControl, EntityId, ExecutionId, IdempotencyKey, ProjectId, ProposalAction,
    RequestId, RevisionId, WorkspaceId,
};
use jingwei_kernel::ToolResult;
use jingwei_schematic::{
    Label, Net, NetClass, PinElectricalType, PinInstance, PointMm, PowerRole, SchematicGraph,
    Sheet, SymbolInstance, Wire,
};
use serde_json::{Value, json};
use tempfile::tempdir;

#[tokio::test]
async fn golden_schematic_is_created_rendered_edited_projected_and_round_tripped() {
    let directory = tempdir().unwrap();
    let workspace = WorkspaceId::new("ws-logical-golden").unwrap();
    let project = ProjectId::new("project-golden-rs485").unwrap();
    let daemon = Arc::new(
        Daemon::create(
            directory.path(),
            workspace.clone(),
            production_registry().unwrap(),
        )
        .unwrap(),
    );
    let client = InMemoryClient::connect(daemon, "logical-golden-test").unwrap();
    let created: JdpResponse<ProjectInfo> = client
        .send(request(
            &workspace,
            &project,
            operation::PROJECT_CREATE,
            "create",
            None,
            json!(ProjectCreateInput {
                name: "温湿度 RS-485 采集板".into()
            }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(created) = created else {
        panic!("project create failed")
    };

    let schematic = golden_schematic();
    let round_trip = SchematicGraph::from_jir_graph(&schematic.to_jir_graph().unwrap()).unwrap();
    assert_eq!(round_trip, schematic);
    let candidate: JdpResponse<ToolResult> = client
        .send(request(
            &workspace,
            &project,
            "schematic_create",
            "schematic-create",
            Some(created.data.current_revision.clone()),
            json!({"graph": schematic, "baseRevision": created.data.current_revision}),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(candidate) = candidate else {
        panic!("schematic_create failed")
    };
    assert!(candidate.data.candidate_operations.len() > 30);
    assert_eq!(candidate.data.data["candidateOnly"], true);

    let applied: JdpResponse<PatchOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::DESIGN_PATCH_APPLY,
            "apply",
            Some(created.data.current_revision.clone()),
            json!(PatchApplyInput {
                proposal_id: None,
                patch: PatchInput {
                    operations: candidate.data.candidate_operations,
                    preconditions: Vec::new(),
                    expected_postconditions: vec!["graph.integrity".into()],
                },
            }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(applied) = applied else {
        panic!("logical candidate apply failed")
    };
    assert_ne!(applied.data.revision, created.data.current_revision);

    let accepted: JdpResponse<ProposalDecisionOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::CHANGE_PROPOSAL_DECIDE,
            "accept",
            None,
            json!(ProposalDecisionInput {
                proposal_id: applied.data.proposal_id,
                action: ProposalAction::Accept,
                reason: Some("golden logical acceptance gate".into()),
            }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(accepted) = accepted else {
        panic!("logical candidate accept failed")
    };
    assert_eq!(accepted.data.project_head, applied.data.revision);

    let validation: JdpResponse<ValidationOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::DESIGN_VALIDATE,
            "validate",
            None,
            json!(ValidateInput {
                revision: Some(applied.data.revision.clone()),
                scopes: vec!["logical".into(), "erc".into()],
            }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(validation) = validation else {
        panic!("logical validation failed")
    };
    assert!(validation.data.valid, "{:?}", validation.data.findings);
    assert!(
        validation
            .data
            .providers_run
            .iter()
            .any(|provider| provider.starts_with("jingwei-native-erc@"))
    );

    let display = view(&client, &workspace, &project, &applied.data.revision).await;
    assert!(display.primitives.len() >= 20);
    let original_u1 = glyph_position(&display, "symbol:u1");
    let hit: JdpResponse<jingwei_jdp::HitTestResult> = client
        .send(request(
            &workspace,
            &project,
            operation::DESIGN_HIT_TEST,
            "hit",
            None,
            json!(DesignHitTestInput {
                provider: "logical.schematic-view".into(),
                request: HitTestRequest {
                    revision: applied.data.revision.clone(),
                    point: original_u1,
                    tolerance: 2.0,
                    layers: vec!["logical".into()],
                },
            }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(hit) = hit else {
        panic!("logical hit test failed")
    };
    assert_eq!(hit.data.hits[0].entity_id.as_str(), "symbol:u1");

    let projected: JdpResponse<DesignProjectOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::DESIGN_PROJECT,
            "project-kicad",
            None,
            json!(DesignProjectInput {
                provider: "logical.kicad-schematic".into(),
                revision: applied.data.revision.clone(),
                options: json!({"target": "10.0.5"}),
            }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(projected) = projected else {
        panic!("KiCad projection failed")
    };
    assert_eq!(projected.data.artifact_refs.len(), 5);
    assert_eq!(projected.data.mapping["targetKiCadVersion"], "10.0.5");
    let schematic_bytes = projected_artifact(
        &client,
        &workspace,
        &project,
        &projected.data,
        "design.kicad_sch",
    )
    .await;
    let summary = jingwei_kicad_schematic::parse_native_schematic(&schematic_bytes).unwrap();
    assert_eq!(summary.symbol_count, 4);
    assert_eq!(summary.pin_count, 12);
    let projection_bytes = projected_artifact(
        &client,
        &workspace,
        &project,
        &projected.data,
        "design.projection.json",
    )
    .await;
    let projection = jingwei_kicad_schematic::parse_projection(&projection_bytes).unwrap();
    jingwei_kicad_schematic::verify_projection_equivalence(&schematic, &projection).unwrap();
    assert!(!projection.coverage.kicad_cli_verified);
    if let Some(output) = env::var_os("JINGWEI_LOGICAL_GOLDEN_OUT") {
        materialize_projection(
            &client,
            &workspace,
            &project,
            &projected.data,
            Path::new(&output),
        )
        .await;
    }

    let opened: JdpResponse<EditSessionInfo> = client
        .send(request(
            &workspace,
            &project,
            operation::EDIT_SESSION_OPEN,
            "edit-open",
            None,
            json!(EditSessionOpenInput {
                base_revision: applied.data.revision.clone(),
                lease_seconds: 1_800,
            }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(opened) = opened else {
        panic!("edit session open failed")
    };
    let moved = Point { x: 320.0, y: 215.0 };
    let mut group = EditCommandGroup {
        edit_session_id: opened.data.edit_session_id.clone(),
        base_draft_revision: opened.data.draft_head_revision.clone(),
        commands: vec![SemanticEditCommand {
            kind: "schematic.move-symbol".into(),
            payload: json!({"symbolId": "symbol:u1", "position": moved}),
        }],
        expected_postconditions: vec!["graph.integrity".into(), "native-erc".into()],
        preview_hash: None,
    };
    let preview: JdpResponse<EditPreviewOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::EDIT_COMMAND_PREVIEW,
            "move-preview",
            None,
            json!(group),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(preview) = preview else {
        panic!("move preview failed")
    };
    assert!(preview.data.validation.valid);
    group.preview_hash = Some(preview.data.preview_hash);
    let committed: JdpResponse<EditCommitOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::EDIT_COMMAND_COMMIT,
            "move-commit",
            None,
            json!(group),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(committed) = committed else {
        panic!("move commit failed")
    };
    assert_eq!(
        glyph_position(
            &view(
                &client,
                &workspace,
                &project,
                &committed.data.draft_revision
            )
            .await,
            "symbol:u1"
        ),
        moved
    );

    let undone: JdpResponse<EditCommitOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::EDIT_UNDO,
            "undo",
            None,
            json!(EditSessionRef {
                edit_session_id: opened.data.edit_session_id.clone(),
            }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(undone) = undone else {
        panic!("undo failed")
    };
    assert_eq!(
        glyph_position(
            &view(&client, &workspace, &project, &undone.data.draft_revision).await,
            "symbol:u1"
        ),
        original_u1
    );
    let redone: JdpResponse<EditCommitOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::EDIT_REDO,
            "redo",
            None,
            json!(EditSessionRef {
                edit_session_id: opened.data.edit_session_id,
            }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(redone) = redone else {
        panic!("redo failed")
    };
    assert_eq!(
        glyph_position(
            &view(&client, &workspace, &project, &redone.data.draft_revision).await,
            "symbol:u1"
        ),
        moved
    );
    let project_info: JdpResponse<ProjectInfo> = client
        .send(request(
            &workspace,
            &project,
            operation::PROJECT_GET,
            "project-after-draft",
            None,
            json!({}),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(project_info) = project_info else {
        panic!("project read after draft failed")
    };
    assert_eq!(project_info.data.current_revision, applied.data.revision);
}

async fn materialize_projection(
    client: &InMemoryClient,
    workspace: &WorkspaceId,
    project: &ProjectId,
    projected: &DesignProjectOutput,
    output: &Path,
) {
    fs::create_dir_all(output).unwrap();
    for artifact in projected.mapping["artifacts"].as_array().unwrap() {
        let name = artifact["name"].as_str().unwrap();
        let bytes = projected_artifact(client, workspace, project, projected, name).await;
        fs::write(output.join(name), bytes).unwrap();
    }
    fs::write(output.join("design.kicad_pro"), b"{}\n").unwrap();
}

async fn projected_artifact(
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
    let response: JdpResponse<ArtifactGetOutput> = client
        .send(request(
            workspace,
            project,
            operation::ARTIFACT_GET,
            &format!("artifact-{}", name.replace(['.', '-'], "-")),
            None,
            json!(ArtifactGetInput { artifact_id }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(response) = response else {
        panic!("projected artifact read failed for {name}")
    };
    hex::decode(response.data.bytes_hex).unwrap()
}

async fn view(
    client: &InMemoryClient,
    workspace: &WorkspaceId,
    project: &ProjectId,
    revision: &RevisionId,
) -> DisplayList {
    let response: JdpResponse<DisplayList> = client
        .send(request(
            workspace,
            project,
            operation::DESIGN_VIEW,
            &format!(
                "view-{}",
                revision.as_str().chars().take(12).collect::<String>()
            ),
            None,
            json!(DesignViewInput {
                provider: "logical.schematic-view".into(),
                request: ViewRequest {
                    revision: revision.clone(),
                    viewport: Rect {
                        min: Point { x: 0.0, y: 0.0 },
                        max: Point {
                            x: 1_000.0,
                            y: 1_000.0
                        },
                    },
                    layers: vec!["logical".into()],
                    level_of_detail: 2,
                    selection: Vec::new(),
                },
            }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(response) = response else {
        panic!("design.view failed")
    };
    response.data
}

fn glyph_position(display: &DisplayList, entity: &str) -> Point {
    display
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            DisplayPrimitive::Glyph {
                position,
                entity_id,
                ..
            } if entity_id.as_str() == entity => Some(*position),
            _ => None,
        })
        .unwrap()
}

fn golden_schematic() -> SchematicGraph {
    let sheet = id("sheet:root");
    let definitions = [
        (
            "symbol:u1",
            "U1",
            "JW-MCU-32A",
            PointMm {
                x: 220.98,
                y: 210.82,
            },
            [
                ("1", "I2C_SCL"),
                ("2", "I2C_SDA"),
                ("3", "UART_TX"),
                ("4", "UART_RX"),
            ]
            .as_slice(),
        ),
        (
            "symbol:u2",
            "U2",
            "JW-TH-44A",
            PointMm {
                x: 469.9,
                y: 210.82,
            },
            [("1", "I2C_SCL"), ("2", "I2C_SDA")].as_slice(),
        ),
        (
            "symbol:u3",
            "U3",
            "JW-RS485-8A",
            PointMm {
                x: 469.9,
                y: 429.26,
            },
            [
                ("1", "UART_TX"),
                ("2", "UART_RX"),
                ("3", "RS485_A"),
                ("4", "RS485_B"),
            ]
            .as_slice(),
        ),
        (
            "symbol:j1",
            "J1",
            "JW-TERM-4A",
            PointMm {
                x: 759.46,
                y: 429.26,
            },
            [("1", "RS485_A"), ("2", "RS485_B")].as_slice(),
        ),
    ];
    let mut symbols = BTreeMap::new();
    let mut nets = [
        "I2C_SCL", "I2C_SDA", "UART_TX", "UART_RX", "RS485_A", "RS485_B",
    ]
    .into_iter()
    .map(|name| {
        let net_id = id(&format!("net:{name}"));
        (
            name,
            Net {
                id: net_id,
                sheet_id: sheet.clone(),
                name: name.into(),
                class: if name.starts_with("RS485") {
                    NetClass::Interface
                } else {
                    NetClass::Signal
                },
                pin_ids: BTreeSet::new(),
                voltage: None,
                current_budget_a: None,
                requires_test_point: false,
                evidence_refs: vec!["fixture:golden-rs485".into()],
            },
        )
    })
    .collect::<BTreeMap<_, _>>();
    for (symbol_id, reference, mpn, position, pins) in definitions {
        let symbol_id = id(symbol_id);
        let mut symbol_pins = BTreeMap::new();
        for (number, net_name) in pins {
            let pin_id = id(&format!("pin:{reference}:{number}"));
            let net = nets.get_mut(net_name).unwrap();
            net.pin_ids.insert(pin_id.clone());
            symbol_pins.insert(
                pin_id.clone(),
                PinInstance {
                    id: pin_id,
                    symbol_id: symbol_id.clone(),
                    number: (*number).into(),
                    name: (*net_name).into(),
                    electrical_type: PinElectricalType::Passive,
                    power_role: PowerRole::None,
                    required_connection: true,
                    net_id: Some(net.id.clone()),
                    allowed_voltage: None,
                    power_domain: None,
                    interface_id: None,
                    properties: BTreeMap::new(),
                    evidence_refs: vec!["fixture:golden-rs485".into()],
                },
            );
        }
        symbols.insert(
            symbol_id.clone(),
            SymbolInstance {
                id: symbol_id,
                sheet_id: sheet.clone(),
                module_id: None,
                reference: reference.into(),
                value: mpn.into(),
                exact_mpn: mpn.into(),
                part_decision_id: format!("part-decision:{reference}"),
                symbol_asset_id: "Connector_Generic:Conn_01x04".into(),
                // The logical golden fixture does not claim a footprint library asset.
                // Physical footprint approval belongs to the physical golden candidate.
                footprint_asset_id: None,
                position,
                rotation_deg: 0.0,
                mirrored: false,
                pins: symbol_pins,
                properties: BTreeMap::from([(
                    "library.verification".into(),
                    "verified-exact".into(),
                )]),
                evidence_refs: vec!["fixture:golden-rs485".into()],
                edit_control: EditControl::default(),
            },
        );
    }
    let nets = nets
        .into_values()
        .map(|net| (net.id.clone(), net))
        .collect::<BTreeMap<_, _>>();
    let wires = [
        ("I2C_SCL", (220.0, 195.0), (470.0, 195.0)),
        ("I2C_SDA", (220.0, 205.0), (470.0, 205.0)),
        ("UART_TX", (220.0, 220.0), (470.0, 415.0)),
        ("UART_RX", (220.0, 230.0), (470.0, 425.0)),
        ("RS485_A", (470.0, 440.0), (760.0, 415.0)),
        ("RS485_B", (470.0, 450.0), (760.0, 435.0)),
    ]
    .into_iter()
    .map(|(name, from, to)| {
        let wire = Wire {
            id: id(&format!("wire:{name}")),
            sheet_id: sheet.clone(),
            net_id: id(&format!("net:{name}")),
            points: vec![
                PointMm {
                    x: from.0,
                    y: from.1,
                },
                PointMm { x: to.0, y: to.1 },
            ],
        };
        (wire.id.clone(), wire)
    })
    .collect();
    let labels = [
        "I2C_SCL", "I2C_SDA", "UART_TX", "UART_RX", "RS485_A", "RS485_B",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, name)| {
        let index = f64::from(u32::try_from(index).unwrap());
        let label = Label {
            id: id(&format!("label:{name}")),
            sheet_id: sheet.clone(),
            net_id: id(&format!("net:{name}")),
            text: name.into(),
            kind: "net".into(),
            position: PointMm {
                x: 335.0 + index * 35.0,
                y: 170.0 + index * 55.0,
            },
        };
        (label.id.clone(), label)
    })
    .collect();
    SchematicGraph {
        id: id("schematic:golden-rs485"),
        schema_version: jingwei_schematic::SCHEMATIC_SCHEMA_VERSION.into(),
        sheets: BTreeMap::from([(
            sheet.clone(),
            Sheet {
                id: sheet,
                parent_sheet_id: None,
                name: "温湿度 RS-485 采集板".into(),
                hierarchical_path: "/".into(),
            },
        )]),
        symbols,
        nets,
        wires,
        labels,
        ..SchematicGraph::default()
    }
}

fn id(value: &str) -> EntityId {
    EntityId::new(value).unwrap()
}

fn request(
    workspace: &WorkspaceId,
    project: &ProjectId,
    operation_name: &str,
    key: &str,
    base_revision: Option<RevisionId>,
    input: Value,
) -> JdpRequest<Value> {
    JdpRequest {
        protocol_version: JDP_PROTOCOL_VERSION.into(),
        request_id: RequestId::new(format!("request-{key}")).unwrap(),
        idempotency_key: IdempotencyKey::new(format!("idem-{key}")).unwrap(),
        workspace_id: workspace.clone(),
        project_id: project.clone(),
        operation: operation_name.into(),
        execution_id: Some(ExecutionId::new(format!("execution-{key}")).unwrap()),
        task_id: None,
        base_revision,
        deadline_ms: Some(30_000),
        dry_run: false,
        trace_context: None,
        input,
    }
}
