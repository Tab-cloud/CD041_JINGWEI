//! PCB candidate domain for Jingwei.
//!
//! This crate consumes the frozen logical-netlist fixture shape from task 03.
//! It returns candidate values only: moving a project revision pointer remains
//! the responsibility of the task-02 kernel transaction service.

mod edit_view;
mod logical;
mod model;

pub use edit_view::{
    CrossProbeResult, DisplayGeometry, DisplayItem, DisplayList, EditCandidate, EditPreview,
    HitTestResult, SemanticEditCommand, ViewRequest,
};
pub use logical::{
    ComponentInstanceRef, FootprintDefinitionRef, LogicalConstraint, LogicalNet, LogicalNetlist,
    LogicalPinRef, PadDefinition,
};
pub use model::{
    Board, BoardDefinition, BoardRegion, CopperLayer, EditControl, Footprint, Keepout, LayerKind,
    MechanicalFeature, NetClass, Pad, PcbError, PcbRules, Side, Stackup, TextItem, Track, Via,
    Zone,
};

/// Frozen logical fixture version consumed before task-03 integration.
pub const LOGICAL_FIXTURE_VERSION: &str = "logical-netlist-v0";
/// PCB candidate schema version.
pub const PCB_CANDIDATE_SCHEMA_VERSION: &str = "pcb-candidate-v0.1";
