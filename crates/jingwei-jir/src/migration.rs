//! Explicit, deterministic JIR schema migration registry.

use std::{collections::BTreeMap, sync::Arc};

use crate::RevisionSnapshot;

/// One deterministic adjacent schema migration.
pub trait SchemaMigration: Send + Sync {
    /// Source schema version.
    fn source_version(&self) -> &'static str;
    /// Target schema version.
    fn target_version(&self) -> &'static str;
    /// Migrates a snapshot without mutating the source object.
    fn migrate(&self, snapshot: RevisionSnapshot) -> Result<RevisionSnapshot, MigrationError>;
}

/// Ordered migration registry. Ambiguous outgoing migrations are rejected.
#[derive(Default)]
pub struct MigrationRegistry {
    migrations: BTreeMap<String, Arc<dyn SchemaMigration>>,
}

impl MigrationRegistry {
    /// Registers one migration for a source version.
    pub fn register<M: SchemaMigration + 'static>(
        &mut self,
        migration: M,
    ) -> Result<(), MigrationError> {
        let from = migration.source_version().to_owned();
        if from == migration.target_version() {
            return Err(MigrationError::Cycle(from));
        }
        if self.migrations.contains_key(&from) {
            return Err(MigrationError::Ambiguous(from));
        }
        self.migrations.insert(from, Arc::new(migration));
        Ok(())
    }

    /// Migrates through adjacent registered versions to `target`.
    pub fn migrate(
        &self,
        mut snapshot: RevisionSnapshot,
        target: &str,
    ) -> Result<RevisionSnapshot, MigrationError> {
        let mut steps = 0;
        while snapshot.metadata.schema_version != target {
            steps += 1;
            if steps > 32 {
                return Err(MigrationError::Cycle(snapshot.metadata.schema_version));
            }
            let migration = self
                .migrations
                .get(&snapshot.metadata.schema_version)
                .ok_or_else(|| MigrationError::Missing {
                    from: snapshot.metadata.schema_version.clone(),
                    to: target.to_owned(),
                })?;
            snapshot = migration.migrate(snapshot)?;
            migration
                .target_version()
                .clone_into(&mut snapshot.metadata.schema_version);
            snapshot
                .reidentify()
                .map_err(|error| MigrationError::Failed(error.to_string()))?;
        }
        Ok(snapshot)
    }
}

/// Migration registration or execution failure.
#[derive(Clone, Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("no migration path from {from} to {to}")]
    Missing { from: String, to: String },
    #[error("multiple migrations leave schema {0}")]
    Ambiguous(String),
    #[error("migration cycle detected at schema {0}")]
    Cycle(String),
    #[error("migration failed: {0}")]
    Failed(String),
}

#[cfg(test)]
mod tests {
    use crate::{JirGraph, ProjectId, RevisionSnapshot};

    use super::*;

    struct RenameSchema;

    impl SchemaMigration for RenameSchema {
        fn source_version(&self) -> &'static str {
            "0.0"
        }
        fn target_version(&self) -> &'static str {
            "0.1"
        }

        fn migrate(
            &self,
            mut snapshot: RevisionSnapshot,
        ) -> Result<RevisionSnapshot, MigrationError> {
            snapshot.metadata.schema_version = self.target_version().into();
            Ok(snapshot)
        }
    }

    #[test]
    fn migrates_only_along_explicit_path() {
        let mut snapshot = RevisionSnapshot::build(
            ProjectId::new("project-migration").unwrap(),
            Vec::new(),
            JirGraph::default(),
            crate::canonical_sha256(&"journal").unwrap(),
        )
        .unwrap();
        snapshot.metadata.schema_version = "0.0".into();
        let mut registry = MigrationRegistry::default();
        registry.register(RenameSchema).unwrap();
        let migrated = registry.migrate(snapshot, "0.1").unwrap();
        assert_eq!(migrated.metadata.schema_version, "0.1");
    }
}
