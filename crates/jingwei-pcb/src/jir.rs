use std::collections::BTreeMap;

use jingwei_jir::{
    CanonicalValue, EdgeId, EditControl as JirEditControl, EntityId, EntityKind, JirEdge,
    JirEntity, JirGraph, Provenance,
};
use serde::Serialize;
use serde_json::json;

use crate::{Board, PcbError};

/// Stable JIR identity of the one authoritative physical board candidate.
pub const PCB_ROOT_ENTITY_ID: &str = "physical:board:main";

impl Board {
    /// Converts the complete PCB candidate into namespaced JIR entities.
    ///
    /// The root retains the full typed board for lossless reconstruction. Addressable
    /// child mirrors provide stable query, explanation and cross-probe identities and
    /// are checked for exact equality on import.
    pub fn to_jir_graph(&self) -> Result<JirGraph, PcbError> {
        let mut graph = JirGraph::default();
        let root_id = entity_id(PCB_ROOT_ENTITY_ID)?;
        insert_entity(
            &mut graph,
            root_id.clone(),
            "physical.board",
            json!({"schemaVersion": self.schema_version, "board": self}),
            self.evidence_refs.clone(),
        )?;
        for (kind, id, value) in child_values(self)? {
            let child_id = entity_id(&id)?;
            insert_entity(&mut graph, child_id.clone(), kind, value, Vec::new())?;
            insert_contains(&mut graph, &root_id, &child_id)?;
        }
        graph
            .validate()
            .map_err(|error| PcbError::JirConversion(error.to_string()))?;
        Ok(graph)
    }

    /// Reconstructs and verifies the physical candidate embedded in a mixed JIR graph.
    ///
    /// Absence is not an error because logical-only revisions are valid. More than one
    /// board root or any stale child mirror is rejected.
    pub fn from_jir_graph(graph: &JirGraph) -> Result<Option<Self>, PcbError> {
        let roots = graph
            .entities
            .values()
            .filter(|entity| entity.kind.as_str() == "physical.board")
            .collect::<Vec<_>>();
        if roots.is_empty() {
            return Ok(None);
        }
        if roots.len() != 1 {
            return Err(PcbError::JirConversion(format!(
                "expected at most one physical.board root, found {}",
                roots.len()
            )));
        }
        let root = roots[0];
        if root.id.as_str() != PCB_ROOT_ENTITY_ID {
            return Err(PcbError::JirConversion(format!(
                "physical board root must be {PCB_ROOT_ENTITY_ID}"
            )));
        }
        let board: Self = serde_json::from_value(
            root.properties
                .as_value()
                .get("board")
                .cloned()
                .ok_or_else(|| PcbError::JirConversion("board payload is missing".to_owned()))?,
        )
        .map_err(|error| PcbError::JirConversion(error.to_string()))?;
        if !board.definition.outline.validity().valid {
            return Err(PcbError::JirConversion(
                "embedded board outline is invalid".to_owned(),
            ));
        }
        let expected = child_values(&board)?
            .into_iter()
            .map(|(kind, id, value)| (id, (kind, value)))
            .collect::<BTreeMap<_, _>>();
        for entity in graph
            .entities
            .values()
            .filter(|entity| entity.kind.as_str().starts_with("physical.") && entity.id != root.id)
        {
            let Some((expected_kind, expected_value)) = expected.get(entity.id.as_str()) else {
                return Err(PcbError::JirConversion(format!(
                    "unknown physical child mirror {}",
                    entity.id
                )));
            };
            if entity.kind.as_str() != *expected_kind
                || entity.properties.as_value() != expected_value
            {
                return Err(PcbError::JirConversion(format!(
                    "stale physical child mirror {}",
                    entity.id
                )));
            }
        }
        if expected.keys().any(|id| {
            graph
                .entities
                .keys()
                .all(|entity_id| entity_id.as_str() != id)
        }) {
            return Err(PcbError::JirConversion(
                "one or more physical child mirrors are missing".to_owned(),
            ));
        }
        Ok(Some(board))
    }
}

fn child_values(board: &Board) -> Result<Vec<(&'static str, String, serde_json::Value)>, PcbError> {
    let mut values = Vec::new();
    extend_serialized(
        &mut values,
        "physical.footprint",
        &board.footprints,
        |value| &value.id,
    )?;
    extend_serialized(&mut values, "physical.pad", &board.pads, |value| &value.id)?;
    extend_serialized(&mut values, "physical.track", &board.tracks, |value| {
        &value.id
    })?;
    extend_serialized(&mut values, "physical.via", &board.vias, |value| &value.id)?;
    extend_serialized(&mut values, "physical.zone", &board.zones, |value| {
        &value.id
    })?;
    extend_serialized(&mut values, "physical.keepout", &board.keepouts, |value| {
        &value.id
    })?;
    extend_serialized(&mut values, "physical.text", &board.texts, |value| {
        &value.id
    })?;
    Ok(values)
}

fn extend_serialized<T: Serialize>(
    values: &mut Vec<(&'static str, String, serde_json::Value)>,
    kind: &'static str,
    items: &[T],
    id: impl Fn(&T) -> &String,
) -> Result<(), PcbError> {
    for item in items {
        values.push((
            kind,
            id(item).clone(),
            serde_json::to_value(item)
                .map_err(|error| PcbError::JirConversion(error.to_string()))?,
        ));
    }
    Ok(())
}

fn insert_entity(
    graph: &mut JirGraph,
    id: EntityId,
    kind: &str,
    value: serde_json::Value,
    evidence_refs: Vec<String>,
) -> Result<(), PcbError> {
    graph
        .insert_entity(JirEntity {
            id,
            kind: EntityKind::new(kind)
                .map_err(|error| PcbError::JirConversion(error.to_string()))?,
            schema_version: crate::PCB_CANDIDATE_SCHEMA_VERSION.to_owned(),
            properties: CanonicalValue::new(value)
                .map_err(|error| PcbError::JirConversion(error.to_string()))?,
            provenance: Provenance {
                source_kind: "physical-domain".to_owned(),
                evidence_refs,
                generator: Some("jingwei-pcb".to_owned()),
                generator_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            },
            edit_control: JirEditControl::default(),
        })
        .map_err(|error| PcbError::JirConversion(error.to_string()))
}

fn insert_contains(
    graph: &mut JirGraph,
    root: &EntityId,
    child: &EntityId,
) -> Result<(), PcbError> {
    graph
        .insert_edge(JirEdge {
            id: EdgeId::new(format!("edge:physical.contains:{root}:{child}"))
                .map_err(|error| PcbError::JirConversion(error.to_string()))?,
            kind: "physical.contains".to_owned(),
            from: root.clone(),
            to: child.clone(),
            properties: CanonicalValue::default(),
        })
        .map_err(|error| PcbError::JirConversion(error.to_string()))
}

fn entity_id(value: &str) -> Result<EntityId, PcbError> {
    EntityId::new(value).map_err(|error| PcbError::JirConversion(error.to_string()))
}

#[cfg(test)]
mod tests {
    use jingwei_geometry::{Length, Point, Polygon};

    use crate::{BoardDefinition, PcbRules, Stackup};

    use super::*;

    #[test]
    fn physical_jir_round_trip_detects_stale_child_mirrors() {
        let board = Board {
            schema_version: crate::PCB_CANDIDATE_SCHEMA_VERSION.to_owned(),
            source_revision: "logical-revision".to_owned(),
            candidate_revision: "physical-revision".to_owned(),
            definition: BoardDefinition {
                outline: Polygon::rectangle(
                    Point::from_millimetres(0, 0),
                    Point::from_millimetres(60, 40),
                ),
                cutouts: Vec::new(),
                mounting_features: Vec::new(),
                stackup: Stackup {
                    board_thickness: Length::from_micrometres(1_600),
                    layers: Vec::new(),
                    surface_finish: "lead-free HASL".to_owned(),
                },
                regions: Vec::new(),
                origin: Point::from_millimetres(0, 0),
            },
            rules: PcbRules::default(),
            footprints: Vec::new(),
            pads: Vec::new(),
            nets: BTreeMap::new(),
            net_priorities: BTreeMap::new(),
            net_classes: Vec::new(),
            tracks: Vec::new(),
            vias: Vec::new(),
            zones: Vec::new(),
            keepouts: Vec::new(),
            texts: Vec::new(),
            pin_pad_map: BTreeMap::new(),
            evidence_refs: Vec::new(),
        };
        let graph = board.to_jir_graph().expect("to JIR");
        assert_eq!(
            Board::from_jir_graph(&graph).expect("from JIR"),
            Some(board)
        );
    }
}
