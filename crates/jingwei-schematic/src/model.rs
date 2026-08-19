use std::collections::{BTreeMap, BTreeSet};

use jingwei_jir::{
    CanonicalValue, EdgeId, EditControl, EntityId, EntityKind, JirEdge, JirEntity, JirGraph,
    Provenance,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

/// Point in schematic millimetres.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PointMm {
    pub x: f64,
    pub y: f64,
}

/// Inclusive electrical voltage range in volts.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoltageRange {
    pub minimum_v: f64,
    pub maximum_v: f64,
}

impl VoltageRange {
    /// Returns true if this range is completely inside `allowed`.
    #[must_use]
    pub fn is_within(self, allowed: Self) -> bool {
        self.minimum_v >= allowed.minimum_v && self.maximum_v <= allowed.maximum_v
    }
}

/// Native pin electrical classification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PinElectricalType {
    Input,
    Output,
    Bidirectional,
    Passive,
    PowerInput,
    PowerOutput,
    OpenCollector,
    NoConnect,
}

/// Explicit power behavior independent of display glyphs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PowerRole {
    None,
    Source,
    Sink,
}

/// Pin instance owned by a symbol instance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinInstance {
    pub id: EntityId,
    pub symbol_id: EntityId,
    pub number: String,
    pub name: String,
    pub electrical_type: PinElectricalType,
    pub power_role: PowerRole,
    pub required_connection: bool,
    pub net_id: Option<EntityId>,
    pub allowed_voltage: Option<VoltageRange>,
    pub power_domain: Option<String>,
    pub interface_id: Option<String>,
    pub properties: BTreeMap<String, String>,
    pub evidence_refs: Vec<String>,
}

/// Placed component symbol with exact part and library links.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInstance {
    pub id: EntityId,
    pub sheet_id: EntityId,
    pub module_id: Option<EntityId>,
    pub reference: String,
    pub value: String,
    pub exact_mpn: String,
    pub part_decision_id: String,
    pub symbol_asset_id: String,
    pub footprint_asset_id: Option<String>,
    pub position: PointMm,
    pub rotation_deg: f64,
    pub mirrored: bool,
    pub pins: BTreeMap<EntityId, PinInstance>,
    pub properties: BTreeMap<String, String>,
    pub evidence_refs: Vec<String>,
    pub edit_control: EditControl,
}

/// Hierarchical sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sheet {
    pub id: EntityId,
    pub parent_sheet_id: Option<EntityId>,
    pub name: String,
    pub hierarchical_path: String,
}

/// Functional module or reusable block.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleBlock {
    pub id: EntityId,
    pub sheet_id: EntityId,
    pub name: String,
    pub requirement_refs: Vec<String>,
}

/// Typed module interface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfacePort {
    pub id: EntityId,
    pub module_id: EntityId,
    pub name: String,
    pub direction: String,
    pub net_id: Option<EntityId>,
}

/// Logical net category.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetClass {
    Signal,
    Power,
    Ground,
    Interface,
}

/// Stable logical connectivity group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Net {
    pub id: EntityId,
    pub sheet_id: EntityId,
    pub name: String,
    pub class: NetClass,
    pub pin_ids: BTreeSet<EntityId>,
    pub voltage: Option<VoltageRange>,
    pub current_budget_a: Option<f64>,
    pub requires_test_point: bool,
    pub evidence_refs: Vec<String>,
}

/// Display wire whose endpoints reference logical entities.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wire {
    pub id: EntityId,
    pub sheet_id: EntityId,
    pub net_id: EntityId,
    pub points: Vec<PointMm>,
}

/// Named bus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bus {
    pub id: EntityId,
    pub sheet_id: EntityId,
    pub name: String,
    pub member_net_ids: Vec<EntityId>,
}

/// Net, global or hierarchical label.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: EntityId,
    pub sheet_id: EntityId,
    pub net_id: EntityId,
    pub text: String,
    pub kind: String,
    pub position: PointMm,
}

/// Explicitly unconnected pin marker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoConnect {
    pub id: EntityId,
    pub pin_id: EntityId,
    pub evidence_refs: Vec<String>,
}

/// DFT test point on a net.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestPoint {
    pub id: EntityId,
    pub net_id: EntityId,
    pub reference: String,
    pub position: PointMm,
    pub requirement_refs: Vec<String>,
}

/// Complete native logical graph.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchematicGraph {
    pub id: EntityId,
    pub schema_version: String,
    pub sheets: BTreeMap<EntityId, Sheet>,
    pub modules: BTreeMap<EntityId, ModuleBlock>,
    pub interfaces: BTreeMap<EntityId, InterfacePort>,
    pub symbols: BTreeMap<EntityId, SymbolInstance>,
    pub nets: BTreeMap<EntityId, Net>,
    pub wires: BTreeMap<EntityId, Wire>,
    pub buses: BTreeMap<EntityId, Bus>,
    pub labels: BTreeMap<EntityId, Label>,
    pub no_connects: BTreeMap<EntityId, NoConnect>,
    pub test_points: BTreeMap<EntityId, TestPoint>,
}

/// Logical graph invariant failure.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum GraphInvariantError {
    #[error("duplicate reference designator: {0}")]
    DuplicateReference(String),
    #[error("object references missing sheet: {0}")]
    MissingSheet(String),
    #[error("pin references missing symbol: {0}")]
    MissingSymbol(String),
    #[error("pin/net connection is not symmetric: {0}")]
    AsymmetricConnection(String),
    #[error("object references missing net: {0}")]
    MissingNet(String),
    #[error("invalid hierarchy path: {0}")]
    InvalidHierarchyPath(String),
    #[error("duplicate stable entity id: {0}")]
    DuplicateEntity(String),
    #[error("JIR conversion failed: {0}")]
    JirConversion(String),
}

impl SchematicGraph {
    /// Checks hierarchy, global stable IDs, reference uniqueness and connectivity symmetry.
    pub fn validate(&self) -> Result<(), GraphInvariantError> {
        let mut ids = BTreeSet::new();
        ids.insert(self.id.clone());
        for id in self
            .sheets
            .keys()
            .chain(self.modules.keys())
            .chain(self.interfaces.keys())
            .chain(self.symbols.keys())
            .chain(self.nets.keys())
            .chain(self.wires.keys())
            .chain(self.buses.keys())
            .chain(self.labels.keys())
            .chain(self.no_connects.keys())
            .chain(self.test_points.keys())
        {
            if !ids.insert(id.clone()) {
                return Err(GraphInvariantError::DuplicateEntity(id.to_string()));
            }
        }
        let mut references = BTreeSet::new();
        let mut all_pins = BTreeMap::new();
        for sheet in self.sheets.values() {
            if !sheet.hierarchical_path.starts_with('/') {
                return Err(GraphInvariantError::InvalidHierarchyPath(
                    sheet.hierarchical_path.clone(),
                ));
            }
            if let Some(parent) = &sheet.parent_sheet_id
                && !self.sheets.contains_key(parent)
            {
                return Err(GraphInvariantError::MissingSheet(parent.to_string()));
            }
        }
        for symbol in self.symbols.values() {
            if !self.sheets.contains_key(&symbol.sheet_id) {
                return Err(GraphInvariantError::MissingSheet(
                    symbol.sheet_id.to_string(),
                ));
            }
            if !references.insert(symbol.reference.clone()) {
                return Err(GraphInvariantError::DuplicateReference(
                    symbol.reference.clone(),
                ));
            }
            for (pin_id, pin) in &symbol.pins {
                if pin_id != &pin.id || pin.symbol_id != symbol.id {
                    return Err(GraphInvariantError::MissingSymbol(pin.id.to_string()));
                }
                if !ids.insert(pin.id.clone()) || all_pins.insert(pin.id.clone(), pin).is_some() {
                    return Err(GraphInvariantError::DuplicateEntity(pin.id.to_string()));
                }
                if let Some(net_id) = &pin.net_id {
                    let Some(net) = self.nets.get(net_id) else {
                        return Err(GraphInvariantError::MissingNet(net_id.to_string()));
                    };
                    if !net.pin_ids.contains(&pin.id) {
                        return Err(GraphInvariantError::AsymmetricConnection(
                            pin.id.to_string(),
                        ));
                    }
                }
            }
        }
        for net in self.nets.values() {
            if !self.sheets.contains_key(&net.sheet_id) {
                return Err(GraphInvariantError::MissingSheet(net.sheet_id.to_string()));
            }
            for pin_id in &net.pin_ids {
                let Some(pin) = all_pins.get(pin_id) else {
                    return Err(GraphInvariantError::AsymmetricConnection(
                        pin_id.to_string(),
                    ));
                };
                if pin.net_id.as_ref() != Some(&net.id) {
                    return Err(GraphInvariantError::AsymmetricConnection(
                        pin_id.to_string(),
                    ));
                }
            }
        }
        for wire in self.wires.values() {
            if !self.nets.contains_key(&wire.net_id) {
                return Err(GraphInvariantError::MissingNet(wire.net_id.to_string()));
            }
        }
        for label in self.labels.values() {
            if !self.nets.contains_key(&label.net_id) {
                return Err(GraphInvariantError::MissingNet(label.net_id.to_string()));
            }
        }
        for test_point in self.test_points.values() {
            if !self.nets.contains_key(&test_point.net_id) {
                return Err(GraphInvariantError::MissingNet(
                    test_point.net_id.to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Finds a pin without exposing a second index as design truth.
    #[must_use]
    pub fn pin(&self, pin_id: &EntityId) -> Option<&PinInstance> {
        self.symbols
            .values()
            .find_map(|symbol| symbol.pins.get(pin_id))
    }

    /// Finds a mutable pin while applying an isolated candidate edit.
    pub(crate) fn pin_mut(&mut self, pin_id: &EntityId) -> Option<&mut PinInstance> {
        self.symbols
            .values_mut()
            .find_map(|symbol| symbol.pins.get_mut(pin_id))
    }

    /// Converts domain objects to namespaced core JIR entities and edges.
    pub fn to_jir_graph(&self) -> Result<JirGraph, GraphInvariantError> {
        self.validate()?;
        let mut result = JirGraph::default();
        insert_entity(
            &mut result,
            self.id.clone(),
            "logical.schematic",
            json!({"schemaVersion": self.schema_version}),
            EditControl::default(),
            Vec::new(),
        )?;
        for sheet in self.sheets.values() {
            insert_serialized(
                &mut result,
                sheet.id.clone(),
                "logical.sheet",
                sheet,
                Vec::new(),
            )?;
            insert_contains(&mut result, &self.id, &sheet.id)?;
        }
        for module in self.modules.values() {
            insert_serialized(
                &mut result,
                module.id.clone(),
                "logical.module",
                module,
                Vec::new(),
            )?;
            insert_contains(&mut result, &module.sheet_id, &module.id)?;
        }
        for symbol in self.symbols.values() {
            let properties = serde_json::to_value(symbol)
                .map_err(|error| GraphInvariantError::JirConversion(error.to_string()))?;
            insert_entity(
                &mut result,
                symbol.id.clone(),
                "logical.symbol",
                properties,
                symbol.edit_control.clone(),
                symbol.evidence_refs.clone(),
            )?;
            insert_contains(&mut result, &symbol.sheet_id, &symbol.id)?;
            for pin in symbol.pins.values() {
                insert_serialized(
                    &mut result,
                    pin.id.clone(),
                    "logical.pin",
                    pin,
                    pin.evidence_refs.clone(),
                )?;
                insert_contains(&mut result, &symbol.id, &pin.id)?;
            }
        }
        for net in self.nets.values() {
            insert_serialized(
                &mut result,
                net.id.clone(),
                "logical.net",
                net,
                net.evidence_refs.clone(),
            )?;
            for pin_id in &net.pin_ids {
                insert_connects(&mut result, pin_id, &net.id)?;
            }
        }
        for test_point in self.test_points.values() {
            insert_serialized(
                &mut result,
                test_point.id.clone(),
                "logical.test-point",
                test_point,
                Vec::new(),
            )?;
            insert_connects(&mut result, &test_point.id, &test_point.net_id)?;
        }
        result
            .validate()
            .map_err(|error| GraphInvariantError::JirConversion(error.to_string()))?;
        Ok(result)
    }
}

fn insert_serialized<T: Serialize>(
    graph: &mut JirGraph,
    id: EntityId,
    kind: &str,
    value: &T,
    evidence_refs: Vec<String>,
) -> Result<(), GraphInvariantError> {
    let properties = serde_json::to_value(value)
        .map_err(|error| GraphInvariantError::JirConversion(error.to_string()))?;
    insert_entity(
        graph,
        id,
        kind,
        properties,
        EditControl::default(),
        evidence_refs,
    )
}

fn insert_entity(
    graph: &mut JirGraph,
    id: EntityId,
    kind: &str,
    value: serde_json::Value,
    edit_control: EditControl,
    evidence_refs: Vec<String>,
) -> Result<(), GraphInvariantError> {
    let kind = EntityKind::new(kind)
        .map_err(|error| GraphInvariantError::JirConversion(error.to_string()))?;
    let properties = CanonicalValue::new(value)
        .map_err(|error| GraphInvariantError::JirConversion(error.to_string()))?;
    graph
        .insert_entity(JirEntity {
            id,
            kind,
            schema_version: crate::SCHEMATIC_SCHEMA_VERSION.to_owned(),
            properties,
            provenance: Provenance {
                source_kind: "logical-domain".to_owned(),
                evidence_refs,
                generator: Some("jingwei-schematic".to_owned()),
                generator_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            },
            edit_control,
        })
        .map_err(|error| GraphInvariantError::JirConversion(error.to_string()))
}

fn insert_contains(
    graph: &mut JirGraph,
    parent: &EntityId,
    child: &EntityId,
) -> Result<(), GraphInvariantError> {
    insert_edge(graph, "logical.contains", parent, child)
}

fn insert_connects(
    graph: &mut JirGraph,
    from: &EntityId,
    to: &EntityId,
) -> Result<(), GraphInvariantError> {
    insert_edge(graph, "logical.connects", from, to)
}

fn insert_edge(
    graph: &mut JirGraph,
    kind: &str,
    from: &EntityId,
    to: &EntityId,
) -> Result<(), GraphInvariantError> {
    let edge_id = EdgeId::new(format!("edge:{kind}:{from}:{to}"))
        .map_err(|error| GraphInvariantError::JirConversion(error.to_string()))?;
    graph
        .insert_edge(JirEdge {
            id: edge_id,
            kind: kind.to_owned(),
            from: from.clone(),
            to: to.clone(),
            properties: CanonicalValue::default(),
        })
        .map_err(|error| GraphInvariantError::JirConversion(error.to_string()))
}
