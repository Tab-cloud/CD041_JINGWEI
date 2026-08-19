//! Manufacturing-candidate export and verification.
//!
//! This crate never creates a Production Release.  Output directories are new,
//! immutable candidates, and release readiness always exposes missing factory,
//! prototype and named-signoff gates.

mod exporter;
mod profile;
mod registration;
mod verifier;

pub use exporter::{
    ExportRequest, Manifest, ManifestEntry, ManufacturingCandidate, ManufacturingError,
    export_candidate,
};
pub use profile::{
    ManufacturerCapabilities, ManufacturerProfile, OutputMapping, ProfileFinding,
    ProfileValidation, load_profile, validate_profile,
};
pub use registration::{
    PhysicalModuleRegistration, PhysicalToolDescriptor, ReleaseReadiness, register_physical_module,
    release_readiness,
};
pub use verifier::{PackageFinding, PackageVerification, verify_candidate};

/// Manufacturing manifest schema.
pub const MANIFEST_SCHEMA_VERSION: &str = "manufacturing-candidate-manifest-v0.1";
