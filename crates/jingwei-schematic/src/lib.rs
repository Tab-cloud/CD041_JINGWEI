//! Logical schematic graph, semantic candidate edits and native display support.
//!
//! No function in this crate writes a store or moves a revision pointer. Edits
//! return candidate operations for the kernel transaction service.

mod edit;
mod model;
mod view;

pub use edit::{
    CandidateOperation, EditError, EditPreview, SemanticEditCommand, apply_candidate,
    preview_commands,
};
pub use model::{
    Bus, GraphInvariantError, InterfacePort, Label, ModuleBlock, Net, NetClass, NoConnect,
    PinElectricalType, PinInstance, PointMm, PowerRole, SchematicGraph, Sheet, SymbolInstance,
    TestPoint, VoltageRange, Wire,
};
pub use view::{
    CrossProbeIndex, CrossProbeLinks, SchematicCrossProbeResult, cross_probe, display_list,
    hit_test,
};

/// Schematic entity schema version registered with JIR.
pub const SCHEMATIC_SCHEMA_VERSION: &str = "schematic-domain/0.1";
