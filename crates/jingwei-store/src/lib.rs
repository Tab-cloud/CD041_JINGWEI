//! Filesystem-backed, single-writer, crash-recoverable workspace storage.

mod artifact;
mod layout;
mod lock;
mod store;

pub use artifact::{ArtifactRecord, MAX_ARTIFACT_BYTES};
pub use lock::WriterLease;
pub use store::{
    CommitRequest, CrashPoint, IdempotencyRecord, OperationJournalRecord, PointerTarget,
    ProjectMetadata, RecoveryReport, StoreError, StoreMigrationReport, StoredEvent,
    WorkspaceMetadata, WorkspaceStore,
};

/// On-disk metadata format version.
pub const STORE_SCHEMA_VERSION: u32 = 2;
