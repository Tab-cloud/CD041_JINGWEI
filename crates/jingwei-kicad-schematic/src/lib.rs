//! Typed KiCad schematic projection model and logical-domain registration.
//!
//! The backend emits both a versioned JSON projection and a native KiCad
//! S-expression schematic. Native output is deterministic and carries a JSON
//! object-map sidecar for JIR-to-KiCad traceability.

mod model;
mod native;
mod projection;
mod registration;

pub use model::{
    KicadNativeArtifact, KicadNativeSummary, KicadNetProjection, KicadObjectMapEntry,
    KicadPinProjection, KicadProjectionModel, KicadSymbolProjection, ProjectionCoverage,
    ProjectionError,
};
pub use native::{
    build_native_artifact, parse_native_schematic, serialize_native_schematic,
    serialize_object_map, serialize_symbol_library, serialize_symbol_library_table,
};
pub use projection::{parse_projection, project_schematic, serialize_projection};
pub use registration::{LogicalModuleRegistration, register_logical_module};

/// Typed model schema version.
pub const KICAD_PROJECTION_SCHEMA_VERSION: &str = "jingwei-kicad-schematic-projection/0.1";
/// Locked reference target required by the product baseline.
pub const KICAD_TARGET_VERSION: &str = "10.0.5";
/// Filename referenced by the generated project-level KiCad symbol table.
pub const KICAD_NATIVE_SYMBOL_LIBRARY_FILE: &str = "jingwei-native-symbols.kicad_sym";
