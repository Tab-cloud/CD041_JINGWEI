//! Provenance-aware library assets, deterministic generation and verification.

mod datasheet;
mod generate;
mod model;
mod resolve;
mod verify;

pub use generate::{
    GenerationError, StructuredFootprintInput, StructuredPinInput, generate_assets,
};
pub use model::{
    ApprovalState, AssetGeometry, AssetKind, AssetSource, FootprintDefinition, LibraryAsset,
    LibraryAssetCandidate, LibraryProvider, PadDefinition, PadShape, PinPadMap, PinPadMapEntry,
    Polarity, ReviewArtifact, SymbolDefinition, SymbolPin, VerificationState,
};
pub use resolve::{LibraryPolicy, LibraryResolution, ResolutionStatus, resolve_library};
pub use verify::{LibraryVerification, verify_library};

/// Library domain schema version.
pub const LIBRARY_SCHEMA_VERSION: &str = "library-domain/0.1";
pub use datasheet::{
    ControlledDatasheetArtifact, DatasheetExtract, DatasheetExtractError, DatasheetPackageFact,
    DatasheetPinFact, extract_controlled_datasheet,
};
