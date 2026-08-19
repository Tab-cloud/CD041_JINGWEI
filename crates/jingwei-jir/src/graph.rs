//! Extensible JIR entity/edge graph with deterministic indices and integrity checks.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CanonicalValue, EdgeId, EntityId, canonical_sha256};

/// Open, namespaced entity kind. Core does not close the domain enum.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct EntityKind(String);

impl EntityKind {
    /// Creates a namespaced entity kind such as `core.project`.
    pub fn new(value: impl Into<String>) -> Result<Self, GraphError> {
        let value = value.into();
        validate_kind(&value)?;
        Ok(Self(value))
    }

    /// Returns the wire name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How an AI or solver may edit an object.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditPolicy {
    /// Automated edits may proceed subject to normal permissions and conditions.
    #[default]
    AiEditable,
    /// An explicit user approval is required.
    AskBeforeChange,
    /// Changes are blocked until the lock is deliberately removed.
    Locked,
}

/// Per-object edit ownership metadata.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditControl {
    /// Current automation policy.
    pub policy: EditPolicy,
    /// Actor that last changed the entity.
    pub last_modified_by: Option<String>,
    /// Operation that last changed the entity.
    pub last_operation_id: Option<String>,
    /// Optional lock owner.
    pub locked_by: Option<String>,
    /// Human-readable lock rationale.
    pub lock_reason: Option<String>,
}

/// Provenance attached to each entity.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    /// Source type, for example `jdl`, `human`, `import`, or `solver`.
    pub source_kind: String,
    /// Immutable evidence references.
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    /// Tool or generator name.
    pub generator: Option<String>,
    /// Tool or generator version.
    pub generator_version: Option<String>,
}

/// Canonical node in the JIR graph.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JirEntity {
    /// Stable entity identity.
    pub id: EntityId,
    /// Open namespaced entity kind.
    pub kind: EntityKind,
    /// Schema version for this entity kind.
    pub schema_version: String,
    /// Canonical properties.
    pub properties: CanonicalValue,
    /// Source and generator provenance.
    pub provenance: Provenance,
    /// Object edit-control metadata.
    #[serde(default)]
    pub edit_control: EditControl,
}

/// Canonical directed relationship in the JIR graph.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JirEdge {
    /// Stable edge identity.
    pub id: EdgeId,
    /// Open namespaced relationship kind.
    pub kind: String,
    /// Source entity.
    pub from: EntityId,
    /// Target entity.
    pub to: EntityId,
    /// Canonical relationship properties.
    pub properties: CanonicalValue,
}

/// Registered schema metadata for an extensible entity or edge kind.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDescriptor {
    /// Namespaced kind.
    pub kind: String,
    /// Current schema version.
    pub schema_version: String,
    /// Optional JSON Schema document.
    pub json_schema: Value,
}

/// Registry used by domain modules to declare namespaced schemas.
#[derive(Clone, Debug, Default)]
pub struct SchemaRegistry {
    entity_schemas: BTreeMap<String, SchemaDescriptor>,
    edge_schemas: BTreeMap<String, SchemaDescriptor>,
}

impl SchemaRegistry {
    /// Registers an entity schema, rejecting duplicate kinds.
    pub fn register_entity(&mut self, descriptor: SchemaDescriptor) -> Result<(), GraphError> {
        validate_kind(&descriptor.kind)?;
        if self.entity_schemas.contains_key(&descriptor.kind) {
            return Err(GraphError::DuplicateSchema(descriptor.kind));
        }
        self.entity_schemas
            .insert(descriptor.kind.clone(), descriptor);
        Ok(())
    }

    /// Registers an edge schema, rejecting duplicate kinds.
    pub fn register_edge(&mut self, descriptor: SchemaDescriptor) -> Result<(), GraphError> {
        validate_kind(&descriptor.kind)?;
        if self.edge_schemas.contains_key(&descriptor.kind) {
            return Err(GraphError::DuplicateSchema(descriptor.kind));
        }
        self.edge_schemas
            .insert(descriptor.kind.clone(), descriptor);
        Ok(())
    }

    /// Looks up an entity schema.
    pub fn entity(&self, kind: &str) -> Option<&SchemaDescriptor> {
        self.entity_schemas.get(kind)
    }

    /// Looks up an edge schema.
    pub fn edge(&self, kind: &str) -> Option<&SchemaDescriptor> {
        self.edge_schemas.get(kind)
    }
}

/// A deterministic graph snapshot. `BTreeMap` ordering is part of canonical serialization.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JirGraph {
    /// Entities indexed by stable ID.
    pub entities: BTreeMap<EntityId, JirEntity>,
    /// Edges indexed by stable ID.
    pub edges: BTreeMap<EdgeId, JirEdge>,
}

impl JirGraph {
    /// Inserts an entity after validating uniqueness and kind syntax.
    pub fn insert_entity(&mut self, entity: JirEntity) -> Result<(), GraphError> {
        validate_kind(entity.kind.as_str())?;
        if self.entities.contains_key(&entity.id) {
            return Err(GraphError::DuplicateEntity(entity.id));
        }
        self.entities.insert(entity.id.clone(), entity);
        Ok(())
    }

    /// Inserts an edge and enforces referential integrity.
    pub fn insert_edge(&mut self, edge: JirEdge) -> Result<(), GraphError> {
        validate_kind(&edge.kind)?;
        if self.edges.contains_key(&edge.id) {
            return Err(GraphError::DuplicateEdge(edge.id));
        }
        if !self.entities.contains_key(&edge.from) {
            return Err(GraphError::MissingEntity(edge.from));
        }
        if !self.entities.contains_key(&edge.to) {
            return Err(GraphError::MissingEntity(edge.to));
        }
        self.edges.insert(edge.id.clone(), edge);
        Ok(())
    }

    /// Removes an edge.
    pub fn remove_edge(&mut self, id: &EdgeId) -> Result<JirEdge, GraphError> {
        self.edges
            .remove(id)
            .ok_or_else(|| GraphError::MissingEdge(id.clone()))
    }

    /// Removes an entity only when no edge references it.
    pub fn remove_entity(&mut self, id: &EntityId) -> Result<JirEntity, GraphError> {
        if let Some(edge) = self
            .edges
            .values()
            .find(|edge| &edge.from == id || &edge.to == id)
        {
            return Err(GraphError::EntityStillReferenced {
                entity: id.clone(),
                edge: edge.id.clone(),
            });
        }
        self.entities
            .remove(id)
            .ok_or_else(|| GraphError::MissingEntity(id.clone()))
    }

    /// Returns entities of an exact namespaced kind.
    pub fn entities_of_kind<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a JirEntity> {
        self.entities
            .values()
            .filter(move |entity| entity.kind.as_str() == kind)
    }

    /// Returns outgoing edges in stable edge-ID order.
    pub fn outgoing<'a>(&'a self, id: &'a EntityId) -> impl Iterator<Item = &'a JirEdge> {
        self.edges.values().filter(move |edge| &edge.from == id)
    }

    /// Checks IDs, kinds, and all edge references.
    pub fn validate(&self) -> Result<(), GraphError> {
        for (id, entity) in &self.entities {
            if id != &entity.id {
                return Err(GraphError::IndexMismatch(id.to_string()));
            }
            validate_kind(entity.kind.as_str())?;
        }
        for (id, edge) in &self.edges {
            if id != &edge.id {
                return Err(GraphError::IndexMismatch(id.to_string()));
            }
            validate_kind(&edge.kind)?;
            if !self.entities.contains_key(&edge.from) {
                return Err(GraphError::MissingEntity(edge.from.clone()));
            }
            if !self.entities.contains_key(&edge.to) {
                return Err(GraphError::MissingEntity(edge.to.clone()));
            }
        }
        Ok(())
    }

    /// Computes a canonical content hash.
    pub fn content_hash(&self) -> Result<String, crate::CanonicalError> {
        canonical_sha256(self)
    }

    /// Computes a stable semantic diff between two graph snapshots.
    pub fn diff(&self, other: &Self) -> GraphDiff {
        let left_entities: BTreeSet<_> = self.entities.keys().cloned().collect();
        let right_entities: BTreeSet<_> = other.entities.keys().cloned().collect();
        let left_edges: BTreeSet<_> = self.edges.keys().cloned().collect();
        let right_edges: BTreeSet<_> = other.edges.keys().cloned().collect();
        let modified_entities = left_entities
            .intersection(&right_entities)
            .filter(|id| self.entities.get(*id) != other.entities.get(*id))
            .cloned()
            .collect();
        let modified_edges = left_edges
            .intersection(&right_edges)
            .filter(|id| self.edges.get(*id) != other.edges.get(*id))
            .cloned()
            .collect();
        GraphDiff {
            added_entities: right_entities.difference(&left_entities).cloned().collect(),
            removed_entities: left_entities.difference(&right_entities).cloned().collect(),
            modified_entities,
            added_edges: right_edges.difference(&left_edges).cloned().collect(),
            removed_edges: left_edges.difference(&right_edges).cloned().collect(),
            modified_edges,
        }
    }
}

/// Semantic graph-level diff.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphDiff {
    pub added_entities: Vec<EntityId>,
    pub removed_entities: Vec<EntityId>,
    pub modified_entities: Vec<EntityId>,
    pub added_edges: Vec<EdgeId>,
    pub removed_edges: Vec<EdgeId>,
    pub modified_edges: Vec<EdgeId>,
}

/// Graph construction or integrity failure.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("entity already exists: {0}")]
    DuplicateEntity(EntityId),
    #[error("edge already exists: {0}")]
    DuplicateEdge(EdgeId),
    #[error("entity does not exist: {0}")]
    MissingEntity(EntityId),
    #[error("edge does not exist: {0}")]
    MissingEdge(EdgeId),
    #[error("entity {entity} is still referenced by edge {edge}")]
    EntityStillReferenced { entity: EntityId, edge: EdgeId },
    #[error("kind must be a lowercase namespaced string: {0}")]
    InvalidKind(String),
    #[error("schema is already registered: {0}")]
    DuplicateSchema(String),
    #[error("graph map key does not match object ID: {0}")]
    IndexMismatch(String),
}

fn validate_kind(value: &str) -> Result<(), GraphError> {
    let valid = value.len() <= 128
        && value.contains('.')
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        });
    if valid {
        Ok(())
    } else {
        Err(GraphError::InvalidKind(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn entity(id: &str) -> JirEntity {
        JirEntity {
            id: EntityId::new(id).unwrap(),
            kind: EntityKind::new("core.test").unwrap(),
            schema_version: "0.1".into(),
            properties: CanonicalValue::new(json!({})).unwrap(),
            provenance: Provenance::default(),
            edit_control: EditControl::default(),
        }
    }

    #[test]
    fn rejects_dangling_edge_and_referenced_removal() {
        let mut graph = JirGraph::default();
        graph.insert_entity(entity("a")).unwrap();
        let edge = JirEdge {
            id: EdgeId::new("e").unwrap(),
            kind: "core.connects".into(),
            from: EntityId::new("a").unwrap(),
            to: EntityId::new("missing").unwrap(),
            properties: CanonicalValue::default(),
        };
        assert!(matches!(
            graph.insert_edge(edge),
            Err(GraphError::MissingEntity(_))
        ));
        graph.insert_entity(entity("b")).unwrap();
        graph
            .insert_edge(JirEdge {
                id: EdgeId::new("e").unwrap(),
                kind: "core.connects".into(),
                from: EntityId::new("a").unwrap(),
                to: EntityId::new("b").unwrap(),
                properties: CanonicalValue::default(),
            })
            .unwrap();
        assert!(matches!(
            graph.remove_entity(&EntityId::new("a").unwrap()),
            Err(GraphError::EntityStillReferenced { .. })
        ));
    }
}
