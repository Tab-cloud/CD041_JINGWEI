//! Explicit extension traits and duplicate-safe registry builder.

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use jingwei_jdp::{
    DisplayList, HitTestRequest, HitTestResult, PatchOperation, SemanticEditCommand,
    ToolDescriptor, ValidationOutput, ViewRequest,
};
use jingwei_jir::{EntityId, RevisionId, SchemaDescriptor};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Bounded context made visible to a domain tool.
#[derive(Clone, Debug)]
pub struct ToolContext {
    pub actor: String,
    pub project_id: jingwei_jir::ProjectId,
    pub revision: RevisionId,
    pub capabilities: Vec<String>,
}

/// Generic tool input envelope. Descriptors provide the concrete schema.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolRequest {
    pub name: String,
    pub input: Value,
}

/// Candidate-only domain tool output. It cannot move a revision pointer.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolResult {
    pub candidate_operations: Vec<PatchOperation>,
    pub data: Value,
    pub artifact_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
}

/// A set of related tools contributed by one domain module.
#[async_trait]
pub trait DomainToolModule: Send + Sync {
    fn module_name(&self) -> &str;
    fn descriptors(&self) -> Vec<ToolDescriptor>;
    async fn execute(
        &self,
        context: ToolContext,
        request: ToolRequest,
    ) -> Result<ToolResult, String>;
}

/// Validation scope passed to a registered provider.
#[derive(Clone, Debug)]
pub struct ValidationContext {
    pub revision: RevisionId,
    pub impacted_entities: Vec<EntityId>,
    pub scopes: Vec<String>,
}

/// Incremental deterministic validator.
#[async_trait]
pub trait ValidationProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn validate(
        &self,
        graph: &jingwei_jir::JirGraph,
        context: &ValidationContext,
    ) -> ValidationOutput;
}

/// Semantic display/hit-test provider.
#[async_trait]
pub trait ViewProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn view(&self, request: &ViewRequest) -> Result<DisplayList, String>;
    async fn hit_test(&self, request: &HitTestRequest) -> Result<HitTestResult, String>;
}

/// Projection request to an external or manufacturing representation.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectionRequest {
    pub backend: String,
    pub revision: RevisionId,
    pub options: Value,
}

/// Projection output references; bytes live in the artifact store.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectionResult {
    pub artifact_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub mapping: Value,
}

/// Deterministic projection backend.
#[async_trait]
pub trait ProjectionBackend: Send + Sync {
    fn name(&self) -> &str;
    async fn project(&self, request: &ProjectionRequest) -> Result<ProjectionResult, String>;
}

/// Translates an extensible semantic command into auditable core patch operations.
pub trait SemanticEditHandler: Send + Sync {
    fn command_kind(&self) -> &str;
    fn translate(
        &self,
        graph: &jingwei_jir::JirGraph,
        command: &SemanticEditCommand,
    ) -> Result<Vec<PatchOperation>, String>;
}

/// Declares which dependency keys become stale after an entity change.
pub trait DependencyInvalidationRule: Send + Sync {
    fn name(&self) -> &str;
    fn invalidated_keys(&self, changed_entities: &[EntityId]) -> Vec<String>;
}

/// Immutable extension registry used by a composed daemon.
#[derive(Default)]
pub struct ExtensionRegistry {
    pub(crate) tools: BTreeMap<String, Arc<dyn DomainToolModule>>,
    pub(crate) validators: BTreeMap<String, Arc<dyn ValidationProvider>>,
    pub(crate) views: BTreeMap<String, Arc<dyn ViewProvider>>,
    pub(crate) projections: BTreeMap<String, Arc<dyn ProjectionBackend>>,
    pub(crate) edit_handlers: BTreeMap<String, Arc<dyn SemanticEditHandler>>,
    pub(crate) invalidation_rules: BTreeMap<String, Arc<dyn DependencyInvalidationRule>>,
    pub(crate) entity_schemas: BTreeMap<String, SchemaDescriptor>,
    pub(crate) edge_schemas: BTreeMap<String, SchemaDescriptor>,
}

impl ExtensionRegistry {
    /// Registered validator names.
    pub fn validator_names(&self) -> Vec<String> {
        self.validators.keys().cloned().collect()
    }

    /// Looks up a semantic edit handler.
    pub fn edit_handler(&self, kind: &str) -> Option<&Arc<dyn SemanticEditHandler>> {
        self.edit_handlers.get(kind)
    }

    /// Returns all tool descriptors in deterministic order.
    pub fn tool_descriptors(&self) -> Vec<ToolDescriptor> {
        self.tools
            .values()
            .flat_map(|module| module.descriptors())
            .collect()
    }
}

/// Fluent, duplicate-safe composition root for domain crates.
#[derive(Default)]
pub struct ExtensionRegistryBuilder {
    registry: ExtensionRegistry,
}

impl ExtensionRegistryBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tool_module(
        mut self,
        module: impl DomainToolModule + 'static,
    ) -> Result<Self, RegistryError> {
        let key = module.module_name().to_owned();
        insert_unique(&mut self.registry.tools, &key, Arc::new(module))?;
        Ok(self)
    }

    pub fn validator(
        mut self,
        provider: impl ValidationProvider + 'static,
    ) -> Result<Self, RegistryError> {
        let key = provider.name().to_owned();
        insert_unique(&mut self.registry.validators, &key, Arc::new(provider))?;
        Ok(self)
    }

    pub fn view_provider(
        mut self,
        provider: impl ViewProvider + 'static,
    ) -> Result<Self, RegistryError> {
        let key = provider.name().to_owned();
        insert_unique(&mut self.registry.views, &key, Arc::new(provider))?;
        Ok(self)
    }

    pub fn projection(
        mut self,
        provider: impl ProjectionBackend + 'static,
    ) -> Result<Self, RegistryError> {
        let key = provider.name().to_owned();
        insert_unique(&mut self.registry.projections, &key, Arc::new(provider))?;
        Ok(self)
    }

    pub fn edit_handler(
        mut self,
        handler: impl SemanticEditHandler + 'static,
    ) -> Result<Self, RegistryError> {
        let key = handler.command_kind().to_owned();
        insert_unique(&mut self.registry.edit_handlers, &key, Arc::new(handler))?;
        Ok(self)
    }

    pub fn invalidation_rule(
        mut self,
        rule: impl DependencyInvalidationRule + 'static,
    ) -> Result<Self, RegistryError> {
        let key = rule.name().to_owned();
        insert_unique(&mut self.registry.invalidation_rules, &key, Arc::new(rule))?;
        Ok(self)
    }

    pub fn entity_schema(mut self, schema: SchemaDescriptor) -> Result<Self, RegistryError> {
        insert_unique(
            &mut self.registry.entity_schemas,
            &schema.kind.clone(),
            schema,
        )?;
        Ok(self)
    }

    pub fn edge_schema(mut self, schema: SchemaDescriptor) -> Result<Self, RegistryError> {
        insert_unique(
            &mut self.registry.edge_schemas,
            &schema.kind.clone(),
            schema,
        )?;
        Ok(self)
    }

    pub fn build(self) -> ExtensionRegistry {
        self.registry
    }
}

fn insert_unique<T>(
    map: &mut BTreeMap<String, T>,
    key: &str,
    value: T,
) -> Result<(), RegistryError> {
    if key.is_empty() || map.contains_key(key) {
        return Err(RegistryError::DuplicateOrEmpty(key.into()));
    }
    map.insert(key.into(), value);
    Ok(())
}

/// Registry composition error.
#[derive(Clone, Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("registry key is empty or duplicated: {0}")]
    DuplicateOrEmpty(String),
}
