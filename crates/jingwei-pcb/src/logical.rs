use jingwei_geometry::{Length, Point};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Frozen, versioned logical-netlist fixture consumed from task 03.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicalNetlist {
    /// Fixture contract version.
    pub schema_version: String,
    /// Revision that produced the logical graph.
    pub source_revision: String,
    /// Component instances with exact footprint bindings.
    pub components: Vec<ComponentInstanceRef>,
    /// Logical pins and their assigned networks.
    pub pins: Vec<LogicalPinRef>,
    /// Stable logical networks.
    pub nets: Vec<LogicalNet>,
    /// Physical constraints projected from the logical design.
    pub constraints: Vec<LogicalConstraint>,
    /// Evidence references inherited by physical findings.
    pub evidence_refs: Vec<String>,
}

/// One logical component instance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentInstanceRef {
    /// Stable logical component ID.
    pub id: String,
    /// Reference designator.
    pub designator: String,
    /// Display value.
    pub value: String,
    /// Exact manufacturer name.
    pub manufacturer: String,
    /// Exact manufacturer part number.
    pub mpn: String,
    /// Exact footprint candidate; missing values block PCB creation.
    pub footprint: Option<FootprintDefinitionRef>,
    /// Logical pin number to physical pad number.
    pub pad_map: BTreeMap<String, String>,
    /// Placement role such as connector, power, critical or ordinary.
    pub placement_role: String,
    /// Whether the upstream contract requires a fixed placement.
    pub locked: bool,
    /// Optional fixed position.
    pub fixed_position: Option<Point>,
}

/// Exact footprint definition embedded in the frozen fixture.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FootprintDefinitionRef {
    /// Source library identifier.
    pub library_id: String,
    /// Exact footprint name.
    pub name: String,
    /// Body width.
    pub body_width: Length,
    /// Body height.
    pub body_height: Length,
    /// Courtyard expansion on all sides.
    pub courtyard_margin: Length,
    /// Optional component height.
    pub component_height: Length,
    /// Physical pads.
    pub pads: Vec<PadDefinition>,
}

/// Physical pad definition relative to the footprint origin.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PadDefinition {
    /// Pad number.
    pub number: String,
    /// Offset relative to the footprint origin.
    pub offset: Point,
    /// Copper width.
    pub width: Length,
    /// Copper height.
    pub height: Length,
    /// Drill diameter, or zero for SMT.
    pub drill: Length,
    /// Whether a drilled pad is plated.
    pub plated: bool,
}

/// Logical pin connection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicalPinRef {
    /// Stable pin ID.
    pub id: String,
    /// Owner component ID.
    pub component_id: String,
    /// Datasheet pin number.
    pub pin_number: String,
    /// Connected net ID.
    pub net_id: String,
}

/// Logical net.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicalNet {
    /// Stable net ID.
    pub id: String,
    /// Human-readable net name.
    pub name: String,
    /// Requested net class.
    pub class_id: String,
    /// Routing priority where a lower number is more important.
    pub priority: u16,
}

/// Logical constraint carried to the PCB domain without evaluating unsupported fields.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicalConstraint {
    /// Stable constraint ID.
    pub id: String,
    /// Constraint kind.
    pub kind: String,
    /// Target logical object IDs.
    pub object_refs: Vec<String>,
    /// Typed or structured value encoded by the frozen fixture.
    pub value: serde_json::Value,
    /// Whether failure is hard-blocking.
    pub hard: bool,
}
