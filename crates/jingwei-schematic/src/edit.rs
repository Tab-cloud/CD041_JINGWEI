use std::collections::BTreeSet;

use jingwei_components::{Finding, FindingCoverage, Severity, SuggestedAction};
use jingwei_jdp::SemanticEditCommand as JdpSemanticEditCommand;
use jingwei_jir::{EditPolicy, EntityId, RevisionId, canonical_sha256};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::{
    Bus, Label, ModuleBlock, Net, NoConnect, PointMm, SchematicGraph, SymbolInstance, TestPoint,
    Wire,
};

/// Typed domain edit. It is later wrapped in the extensible core JDP envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SemanticEditCommand {
    PlaceSymbol {
        symbol: Box<SymbolInstance>,
    },
    RemoveSymbol {
        symbol_id: EntityId,
    },
    MoveSymbol {
        symbol_id: EntityId,
        position: PointMm,
    },
    RotateSymbol {
        symbol_id: EntityId,
        rotation_deg: f64,
    },
    MirrorSymbol {
        symbol_id: EntityId,
        mirrored: bool,
    },
    Connect {
        pin_ids: Vec<EntityId>,
        net: Box<Net>,
    },
    Disconnect {
        pin_ids: Vec<EntityId>,
    },
    AddWire {
        wire: Wire,
    },
    RemoveWire {
        wire_id: EntityId,
    },
    AddBus {
        bus: Bus,
    },
    AddLabel {
        label: Label,
    },
    SetProperty {
        symbol_id: EntityId,
        key: String,
        value: String,
    },
    AddModule {
        module: ModuleBlock,
    },
    AddNoConnect {
        no_connect: NoConnect,
    },
    AddTestPoint {
        test_point: TestPoint,
    },
    SetEditControl {
        entity_id: EntityId,
        policy: EditPolicy,
    },
}

impl SemanticEditCommand {
    /// Encodes the domain payload in the core extensible command envelope.
    pub fn to_jdp_command(&self) -> Result<JdpSemanticEditCommand, EditError> {
        let value = serde_json::to_value(self)
            .map_err(|error| EditError::Serialization(error.to_string()))?;
        let kind = value
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| EditError::Serialization("command kind is missing".to_owned()))?;
        Ok(JdpSemanticEditCommand {
            kind: format!("schematic.{kind}"),
            payload: value,
        })
    }
}

/// Non-mutating edit preview.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditPreview {
    pub preview_hash: String,
    pub normalized_commands: Vec<SemanticEditCommand>,
    pub affected_object_ids: Vec<EntityId>,
    pub invalidated_rules: Vec<String>,
    pub ghost_graph: SchematicGraph,
    pub cheap_findings: Vec<Finding>,
    pub committable: bool,
}

/// Candidate returned to the kernel transaction service.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateOperation {
    pub operation_kind: String,
    pub base_revision: RevisionId,
    pub preview_hash: String,
    pub jdp_commands: Vec<JdpSemanticEditCommand>,
    pub expected_postconditions: Vec<String>,
    pub affected_object_ids: Vec<EntityId>,
    pub validation_scope: Vec<String>,
    pub candidate_graph: SchematicGraph,
}

/// Semantic edit failure.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum EditError {
    #[error("object was not found: {0}")]
    ObjectNotFound(String),
    #[error("object is locked or requires explicit approval: {0}")]
    ObjectProtected(String),
    #[error("object already exists: {0}")]
    ObjectAlreadyExists(String),
    #[error("command would violate graph invariants: {0}")]
    Invariant(String),
    #[error("command serialization failed: {0}")]
    Serialization(String),
    #[error("preview hash does not match the candidate command group")]
    PreviewHashMismatch,
}

/// Applies commands to a clone, returning ghost geometry and cheap diagnostics.
pub fn preview_commands(
    graph: &SchematicGraph,
    commands: &[SemanticEditCommand],
) -> Result<EditPreview, EditError> {
    let mut candidate = graph.clone();
    let mut affected = BTreeSet::new();
    for command in commands {
        apply_one(&mut candidate, command, &mut affected)?;
    }
    let cheap_findings = match candidate.validate() {
        Ok(()) => Vec::new(),
        Err(error) => vec![Finding::new(
            "SCH.GRAPH_INVARIANT",
            "1.0.0",
            Severity::Blocking,
            affected.iter().map(ToString::to_string).collect(),
            Vec::new(),
            error.to_string(),
            vec![SuggestedAction {
                kind: "change-input".to_owned(),
                description: "Change the semantic command so connectivity remains consistent."
                    .to_owned(),
            }],
            FindingCoverage {
                status: "implemented".to_owned(),
                checked_aspects: vec!["hierarchy, stable IDs and connectivity symmetry".to_owned()],
                gaps: Vec::new(),
            },
        )],
    };
    let normalized_commands = commands.to_vec();
    let preview_hash = canonical_sha256(&json!({
        "graphId": graph.id,
        "commands": normalized_commands,
    }))
    .map_err(|error| EditError::Serialization(error.to_string()))?;
    Ok(EditPreview {
        preview_hash,
        normalized_commands,
        affected_object_ids: affected.into_iter().collect(),
        invalidated_rules: vec![
            "native-erc".to_owned(),
            "kicad-schematic-projection".to_owned(),
            "logical-netlist".to_owned(),
        ],
        ghost_graph: candidate,
        committable: cheap_findings.is_empty(),
        cheap_findings,
    })
}

/// Converts an accepted preview into a candidate operation without writing a revision.
pub fn apply_candidate(
    base_revision: RevisionId,
    preview: EditPreview,
    supplied_preview_hash: &str,
) -> Result<CandidateOperation, EditError> {
    if preview.preview_hash != supplied_preview_hash || !preview.committable {
        return Err(EditError::PreviewHashMismatch);
    }
    let jdp_commands = preview
        .normalized_commands
        .iter()
        .map(SemanticEditCommand::to_jdp_command)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CandidateOperation {
        operation_kind: "logical.schematic.apply-change".to_owned(),
        base_revision,
        preview_hash: preview.preview_hash,
        jdp_commands,
        expected_postconditions: vec![
            "logical graph validates".to_owned(),
            "pin/net connectivity is symmetric".to_owned(),
            "protected objects are unchanged".to_owned(),
        ],
        affected_object_ids: preview.affected_object_ids,
        validation_scope: preview.invalidated_rules,
        candidate_graph: preview.ghost_graph,
    })
}

fn apply_one(
    graph: &mut SchematicGraph,
    command: &SemanticEditCommand,
    affected: &mut BTreeSet<EntityId>,
) -> Result<(), EditError> {
    match command {
        SemanticEditCommand::PlaceSymbol { symbol } => {
            if graph.symbols.contains_key(&symbol.id) {
                return Err(EditError::ObjectAlreadyExists(symbol.id.to_string()));
            }
            if !graph.sheets.contains_key(&symbol.sheet_id) {
                return Err(EditError::ObjectNotFound(symbol.sheet_id.to_string()));
            }
            affected.insert(symbol.id.clone());
            graph
                .symbols
                .insert(symbol.id.clone(), symbol.as_ref().clone());
        }
        SemanticEditCommand::RemoveSymbol { symbol_id } => {
            ensure_symbol_editable(graph, symbol_id)?;
            let symbol = graph
                .symbols
                .get(symbol_id)
                .ok_or_else(|| EditError::ObjectNotFound(symbol_id.to_string()))?;
            if symbol.pins.values().any(|pin| pin.net_id.is_some()) {
                return Err(EditError::Invariant(
                    "disconnect all symbol pins before removal".to_owned(),
                ));
            }
            affected.insert(symbol_id.clone());
            graph.symbols.remove(symbol_id);
        }
        SemanticEditCommand::MoveSymbol {
            symbol_id,
            position,
        } => {
            ensure_symbol_editable(graph, symbol_id)?;
            if !position.x.is_finite() || !position.y.is_finite() {
                return Err(EditError::Invariant("position must be finite".to_owned()));
            }
            graph
                .symbols
                .get_mut(symbol_id)
                .ok_or_else(|| EditError::ObjectNotFound(symbol_id.to_string()))?
                .position = *position;
            affected.insert(symbol_id.clone());
        }
        SemanticEditCommand::RotateSymbol {
            symbol_id,
            rotation_deg,
        } => {
            ensure_symbol_editable(graph, symbol_id)?;
            if !rotation_deg.is_finite() {
                return Err(EditError::Invariant("rotation must be finite".to_owned()));
            }
            graph
                .symbols
                .get_mut(symbol_id)
                .ok_or_else(|| EditError::ObjectNotFound(symbol_id.to_string()))?
                .rotation_deg = rotation_deg.rem_euclid(360.0);
            affected.insert(symbol_id.clone());
        }
        SemanticEditCommand::MirrorSymbol {
            symbol_id,
            mirrored,
        } => {
            ensure_symbol_editable(graph, symbol_id)?;
            graph
                .symbols
                .get_mut(symbol_id)
                .ok_or_else(|| EditError::ObjectNotFound(symbol_id.to_string()))?
                .mirrored = *mirrored;
            affected.insert(symbol_id.clone());
        }
        SemanticEditCommand::Connect { pin_ids, net } => {
            if let Some(existing) = graph.nets.get(&net.id)
                && existing.name != net.name
            {
                return Err(EditError::ObjectAlreadyExists(net.id.to_string()));
            }
            graph
                .nets
                .entry(net.id.clone())
                .or_insert_with(|| net.as_ref().clone());
            for pin_id in pin_ids {
                let old_net_id = graph
                    .pin(pin_id)
                    .ok_or_else(|| EditError::ObjectNotFound(pin_id.to_string()))?
                    .net_id
                    .clone();
                if let Some(old_net_id) = old_net_id
                    && old_net_id != net.id
                    && let Some(old_net) = graph.nets.get_mut(&old_net_id)
                {
                    old_net.pin_ids.remove(pin_id);
                }
                graph
                    .pin_mut(pin_id)
                    .ok_or_else(|| EditError::ObjectNotFound(pin_id.to_string()))?
                    .net_id = Some(net.id.clone());
                graph
                    .nets
                    .get_mut(&net.id)
                    .expect("net was inserted above")
                    .pin_ids
                    .insert(pin_id.clone());
                affected.insert(pin_id.clone());
            }
            affected.insert(net.id.clone());
        }
        SemanticEditCommand::Disconnect { pin_ids } => {
            for pin_id in pin_ids {
                let net_id = graph
                    .pin(pin_id)
                    .ok_or_else(|| EditError::ObjectNotFound(pin_id.to_string()))?
                    .net_id
                    .clone();
                if let Some(net_id) = net_id {
                    if let Some(net) = graph.nets.get_mut(&net_id) {
                        net.pin_ids.remove(pin_id);
                    }
                    graph.pin_mut(pin_id).expect("pin checked above").net_id = None;
                    affected.insert(net_id);
                }
                affected.insert(pin_id.clone());
            }
        }
        SemanticEditCommand::AddWire { wire } => {
            if graph.wires.contains_key(&wire.id) {
                return Err(EditError::ObjectAlreadyExists(wire.id.to_string()));
            }
            if !graph.nets.contains_key(&wire.net_id) || wire.points.len() < 2 {
                return Err(EditError::Invariant(
                    "wire requires an existing net and at least two points".to_owned(),
                ));
            }
            affected.insert(wire.id.clone());
            affected.insert(wire.net_id.clone());
            graph.wires.insert(wire.id.clone(), wire.clone());
        }
        SemanticEditCommand::RemoveWire { wire_id } => {
            let wire = graph
                .wires
                .remove(wire_id)
                .ok_or_else(|| EditError::ObjectNotFound(wire_id.to_string()))?;
            affected.insert(wire.id);
            affected.insert(wire.net_id);
        }
        SemanticEditCommand::AddBus { bus } => {
            if bus
                .member_net_ids
                .iter()
                .any(|net_id| !graph.nets.contains_key(net_id))
            {
                return Err(EditError::Invariant(
                    "bus member nets must already exist".to_owned(),
                ));
            }
            affected.insert(bus.id.clone());
            graph.buses.insert(bus.id.clone(), bus.clone());
        }
        SemanticEditCommand::AddLabel { label } => {
            if !graph.nets.contains_key(&label.net_id) {
                return Err(EditError::ObjectNotFound(label.net_id.to_string()));
            }
            affected.insert(label.id.clone());
            affected.insert(label.net_id.clone());
            graph.labels.insert(label.id.clone(), label.clone());
        }
        SemanticEditCommand::SetProperty {
            symbol_id,
            key,
            value,
        } => {
            ensure_symbol_editable(graph, symbol_id)?;
            if matches!(key.as_str(), "exactMpn" | "pinMap" | "footprintAssetId") {
                return Err(EditError::Invariant(format!(
                    "{key} requires a typed part/library change service"
                )));
            }
            graph
                .symbols
                .get_mut(symbol_id)
                .ok_or_else(|| EditError::ObjectNotFound(symbol_id.to_string()))?
                .properties
                .insert(key.clone(), value.clone());
            affected.insert(symbol_id.clone());
        }
        SemanticEditCommand::AddModule { module } => {
            if !graph.sheets.contains_key(&module.sheet_id) {
                return Err(EditError::ObjectNotFound(module.sheet_id.to_string()));
            }
            affected.insert(module.id.clone());
            graph.modules.insert(module.id.clone(), module.clone());
        }
        SemanticEditCommand::AddNoConnect { no_connect } => {
            let pin = graph
                .pin(&no_connect.pin_id)
                .ok_or_else(|| EditError::ObjectNotFound(no_connect.pin_id.to_string()))?;
            if pin.net_id.is_some() {
                return Err(EditError::Invariant(
                    "a connected pin cannot be marked no-connect".to_owned(),
                ));
            }
            affected.insert(no_connect.id.clone());
            affected.insert(no_connect.pin_id.clone());
            graph
                .no_connects
                .insert(no_connect.id.clone(), no_connect.clone());
        }
        SemanticEditCommand::AddTestPoint { test_point } => {
            if !graph.nets.contains_key(&test_point.net_id) {
                return Err(EditError::ObjectNotFound(test_point.net_id.to_string()));
            }
            affected.insert(test_point.id.clone());
            affected.insert(test_point.net_id.clone());
            graph
                .test_points
                .insert(test_point.id.clone(), test_point.clone());
        }
        SemanticEditCommand::SetEditControl { entity_id, policy } => {
            let symbol = graph
                .symbols
                .get_mut(entity_id)
                .ok_or_else(|| EditError::ObjectNotFound(entity_id.to_string()))?;
            symbol.edit_control.policy = *policy;
            affected.insert(entity_id.clone());
        }
    }
    Ok(())
}

fn ensure_symbol_editable(graph: &SchematicGraph, symbol_id: &EntityId) -> Result<(), EditError> {
    let symbol = graph
        .symbols
        .get(symbol_id)
        .ok_or_else(|| EditError::ObjectNotFound(symbol_id.to_string()))?;
    if symbol.edit_control.policy == EditPolicy::AiEditable {
        Ok(())
    } else {
        Err(EditError::ObjectProtected(symbol_id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use jingwei_jir::{EditControl, EntityId};

    use super::*;
    use crate::{PinElectricalType, PinInstance, PowerRole, Sheet};

    fn id(value: &str) -> EntityId {
        EntityId::new(value).expect("valid id")
    }

    fn graph() -> SchematicGraph {
        let sheet_id = id("sheet:root");
        let symbol_id = id("symbol:u1");
        let pin_id = id("pin:u1:1");
        let symbol = SymbolInstance {
            id: symbol_id.clone(),
            sheet_id: sheet_id.clone(),
            module_id: None,
            reference: "U1".to_owned(),
            value: "MCU".to_owned(),
            exact_mpn: "JW-MCU-1".to_owned(),
            part_decision_id: "part-decision:mcu".to_owned(),
            symbol_asset_id: "symbol-asset:mcu".to_owned(),
            footprint_asset_id: Some("footprint-asset:mcu".to_owned()),
            position: PointMm { x: 0.0, y: 0.0 },
            rotation_deg: 0.0,
            mirrored: false,
            pins: BTreeMap::from([(
                pin_id.clone(),
                PinInstance {
                    id: pin_id,
                    symbol_id: symbol_id.clone(),
                    number: "1".to_owned(),
                    name: "VDD".to_owned(),
                    electrical_type: PinElectricalType::PowerInput,
                    power_role: PowerRole::Sink,
                    required_connection: true,
                    net_id: None,
                    allowed_voltage: None,
                    power_domain: Some("V3V3".to_owned()),
                    interface_id: None,
                    properties: BTreeMap::new(),
                    evidence_refs: Vec::new(),
                },
            )]),
            properties: BTreeMap::new(),
            evidence_refs: Vec::new(),
            edit_control: EditControl::default(),
        };
        SchematicGraph {
            id: id("schematic:golden"),
            schema_version: crate::SCHEMATIC_SCHEMA_VERSION.to_owned(),
            sheets: BTreeMap::from([(
                sheet_id.clone(),
                Sheet {
                    id: sheet_id,
                    parent_sheet_id: None,
                    name: "Root".to_owned(),
                    hierarchical_path: "/".to_owned(),
                },
            )]),
            symbols: BTreeMap::from([(symbol_id, symbol)]),
            ..SchematicGraph::default()
        }
    }

    #[test]
    fn preview_does_not_mutate_source_graph() {
        let original = graph();
        let symbol_id = id("symbol:u1");
        let preview = preview_commands(
            &original,
            &[SemanticEditCommand::MoveSymbol {
                symbol_id: symbol_id.clone(),
                position: PointMm { x: 10.0, y: 5.0 },
            }],
        )
        .expect("preview");
        assert_eq!(original.symbols[&symbol_id].position.x, 0.0);
        assert_eq!(preview.ghost_graph.symbols[&symbol_id].position.x, 10.0);
        assert!(preview.committable);
    }

    #[test]
    fn locked_symbol_is_preserved() {
        let mut source = graph();
        let symbol_id = id("symbol:u1");
        source
            .symbols
            .get_mut(&symbol_id)
            .expect("symbol")
            .edit_control
            .policy = EditPolicy::Locked;
        let error = preview_commands(
            &source,
            &[SemanticEditCommand::MoveSymbol {
                symbol_id,
                position: PointMm { x: 2.0, y: 0.0 },
            }],
        )
        .expect_err("locked object must fail");
        assert!(matches!(error, EditError::ObjectProtected(_)));
    }
}
