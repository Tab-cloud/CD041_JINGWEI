//! Atomic patch application, inverse construction, and preconditions.

use jingwei_jdp::{PatchOperation, Precondition};
use jingwei_jir::{CanonicalValue, EditPolicy, EntityId, JirGraph};
use serde_json::Value;

use crate::{KernelError, RequestContext};

/// Graph after a successful isolated patch plus an inverse operation sequence.
pub struct AppliedPatch {
    pub graph: JirGraph,
    pub inverse: Vec<PatchOperation>,
    pub impacted_entities: Vec<EntityId>,
}

/// Evaluates every patch precondition without changing the graph.
pub fn evaluate_preconditions(
    graph: &JirGraph,
    conditions: &[Precondition],
    context: &RequestContext,
) -> Result<(), KernelError> {
    for condition in conditions {
        match condition {
            Precondition::EntityExists { entity_id } if !graph.entities.contains_key(entity_id) => {
                return Err(KernelError::Precondition(format!(
                    "entity {entity_id} does not exist"
                )));
            }
            Precondition::EntityAbsent { entity_id } if graph.entities.contains_key(entity_id) => {
                return Err(KernelError::Precondition(format!(
                    "entity {entity_id} already exists"
                )));
            }
            Precondition::PropertyEquals {
                entity_id,
                key,
                expected,
            } => {
                let entity = graph.entities.get(entity_id).ok_or_else(|| {
                    KernelError::Precondition(format!("entity {entity_id} does not exist"))
                })?;
                let actual = entity
                    .properties
                    .as_value()
                    .as_object()
                    .and_then(|object| object.get(key));
                if actual != Some(expected) {
                    return Err(KernelError::Precondition(format!(
                        "property {entity_id}.{key} does not equal the expected value"
                    )));
                }
            }
            Precondition::ObjectEditable { entity_id } => {
                ensure_editable(graph, entity_id, context)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Applies all operations to an isolated clone; any failure discards the clone.
pub fn apply_patch_to_graph(
    graph: &JirGraph,
    operations: &[PatchOperation],
    context: &RequestContext,
) -> Result<AppliedPatch, KernelError> {
    let mut graph = graph.clone();
    let mut inverse = Vec::with_capacity(operations.len());
    let mut impacted = Vec::new();
    for operation in operations {
        match operation {
            PatchOperation::AddEntity { entity } => {
                graph.insert_entity(entity.clone())?;
                impacted.push(entity.id.clone());
                inverse.push(PatchOperation::RemoveEntity {
                    entity_id: entity.id.clone(),
                });
            }
            PatchOperation::RemoveEntity { entity_id } => {
                ensure_editable(&graph, entity_id, context)?;
                let entity = graph.remove_entity(entity_id)?;
                impacted.push(entity_id.clone());
                inverse.push(PatchOperation::AddEntity { entity });
            }
            PatchOperation::SetProperty {
                entity_id,
                key,
                value,
            } => {
                ensure_editable(&graph, entity_id, context)?;
                validate_property_key(key)?;
                let entity = graph
                    .entities
                    .get_mut(entity_id)
                    .ok_or_else(|| jingwei_jir::GraphError::MissingEntity(entity_id.clone()))?;
                let mut properties = entity
                    .properties
                    .as_value()
                    .as_object()
                    .cloned()
                    .ok_or_else(|| {
                        KernelError::InvalidPatch("entity properties must be an object".into())
                    })?;
                let old = properties.insert(key.clone(), value.as_value().clone());
                entity.properties = CanonicalValue::new(Value::Object(properties))?;
                impacted.push(entity_id.clone());
                inverse.push(match old {
                    Some(old) => PatchOperation::SetProperty {
                        entity_id: entity_id.clone(),
                        key: key.clone(),
                        value: CanonicalValue::new(old)?,
                    },
                    None => PatchOperation::RemoveProperty {
                        entity_id: entity_id.clone(),
                        key: key.clone(),
                    },
                });
            }
            PatchOperation::RemoveProperty { entity_id, key } => {
                ensure_editable(&graph, entity_id, context)?;
                validate_property_key(key)?;
                let entity = graph
                    .entities
                    .get_mut(entity_id)
                    .ok_or_else(|| jingwei_jir::GraphError::MissingEntity(entity_id.clone()))?;
                let mut properties = entity
                    .properties
                    .as_value()
                    .as_object()
                    .cloned()
                    .ok_or_else(|| {
                        KernelError::InvalidPatch("entity properties must be an object".into())
                    })?;
                let old = properties.remove(key).ok_or_else(|| {
                    KernelError::InvalidPatch(format!("property {entity_id}.{key} does not exist"))
                })?;
                entity.properties = CanonicalValue::new(Value::Object(properties))?;
                impacted.push(entity_id.clone());
                inverse.push(PatchOperation::SetProperty {
                    entity_id: entity_id.clone(),
                    key: key.clone(),
                    value: CanonicalValue::new(old)?,
                });
            }
            PatchOperation::SetEditPolicy { entity_id, policy } => {
                let entity = graph
                    .entities
                    .get_mut(entity_id)
                    .ok_or_else(|| jingwei_jir::GraphError::MissingEntity(entity_id.clone()))?;
                if entity.edit_control.policy == EditPolicy::Locked && !context.has("edit.unlock") {
                    return Err(KernelError::PermissionDenied("edit.unlock".into()));
                }
                let old = entity.edit_control.policy;
                entity.edit_control.policy = *policy;
                entity.edit_control.last_modified_by = Some(context.actor.clone());
                impacted.push(entity_id.clone());
                inverse.push(PatchOperation::SetEditPolicy {
                    entity_id: entity_id.clone(),
                    policy: old,
                });
            }
            PatchOperation::AddEdge { edge } => {
                ensure_editable(&graph, &edge.from, context)?;
                graph.insert_edge(edge.clone())?;
                impacted.extend([edge.from.clone(), edge.to.clone()]);
                inverse.push(PatchOperation::RemoveEdge {
                    edge_id: edge.id.clone(),
                });
            }
            PatchOperation::RemoveEdge { edge_id } => {
                let edge = graph
                    .edges
                    .get(edge_id)
                    .cloned()
                    .ok_or_else(|| jingwei_jir::GraphError::MissingEdge(edge_id.clone()))?;
                ensure_editable(&graph, &edge.from, context)?;
                graph.remove_edge(edge_id)?;
                impacted.extend([edge.from.clone(), edge.to.clone()]);
                inverse.push(PatchOperation::AddEdge { edge });
            }
        }
    }
    inverse.reverse();
    impacted.sort();
    impacted.dedup();
    graph.validate()?;
    Ok(AppliedPatch {
        graph,
        inverse,
        impacted_entities: impacted,
    })
}

fn ensure_editable(
    graph: &JirGraph,
    entity_id: &EntityId,
    context: &RequestContext,
) -> Result<(), KernelError> {
    let entity = graph
        .entities
        .get(entity_id)
        .ok_or_else(|| jingwei_jir::GraphError::MissingEntity(entity_id.clone()))?;
    match entity.edit_control.policy {
        EditPolicy::AiEditable => Ok(()),
        EditPolicy::AskBeforeChange if context.has("edit.approved") => Ok(()),
        EditPolicy::AskBeforeChange => Err(KernelError::PermissionDenied("edit.approved".into())),
        EditPolicy::Locked => Err(KernelError::PermissionDenied("object is locked".into())),
    }
}

fn validate_property_key(key: &str) -> Result<(), KernelError> {
    if key.is_empty() || key.len() > 128 || key.chars().any(char::is_control) {
        Err(KernelError::InvalidPatch(
            "property key is empty, overlong, or contains control characters".into(),
        ))
    } else {
        Ok(())
    }
}
