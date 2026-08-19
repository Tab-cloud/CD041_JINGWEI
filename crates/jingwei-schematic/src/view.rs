use std::cmp::Ordering;
use std::collections::BTreeMap;

use jingwei_jdp::{
    CrossProbeResult, DisplayList, DisplayPrimitive, Hit, HitTestRequest, HitTestResult, Point,
    ViewRequest,
};
use jingwei_jir::EntityId;
use serde::{Deserialize, Serialize};

use crate::{PointMm, SchematicGraph};

/// Cross-view links maintained by stable JIR identity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossProbeLinks {
    pub related_entity_ids: Vec<EntityId>,
    pub part_decision_refs: Vec<String>,
    pub footprint_asset_refs: Vec<String>,
    pub bom_line_refs: Vec<String>,
    pub finding_refs: Vec<String>,
}

/// Cross-probe lookup table; it is an index, not a second source of truth.
pub type CrossProbeIndex = BTreeMap<EntityId, CrossProbeLinks>;

/// Rich schematic cross-probe result plus the core entity-only envelope.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchematicCrossProbeResult {
    pub core: CrossProbeResult,
    pub part_decision_refs: Vec<String>,
    pub footprint_asset_refs: Vec<String>,
    pub bom_line_refs: Vec<String>,
    pub finding_refs: Vec<String>,
}

/// Builds an LOD-aware core display list for a viewport.
#[must_use]
pub fn display_list(graph: &SchematicGraph, request: &ViewRequest) -> DisplayList {
    let mut primitives = Vec::new();
    for symbol in graph.symbols.values() {
        if !inside(symbol.position, request) {
            continue;
        }
        let centre = point(symbol.position);
        primitives.push(DisplayPrimitive::Glyph {
            position: centre,
            name: "schematic-symbol".to_owned(),
            entity_id: symbol.id.clone(),
        });
        primitives.push(DisplayPrimitive::Text {
            position: Point {
                x: centre.x,
                y: centre.y - 3.0,
            },
            text: format!("{} {}", symbol.reference, symbol.value),
            entity_id: symbol.id.clone(),
        });
        if request.level_of_detail > 0 {
            for (index, pin) in symbol.pins.values().enumerate() {
                primitives.push(DisplayPrimitive::Marker {
                    position: Point {
                        x: centre.x + 5.0,
                        y: centre.y + index as f64 * 2.0,
                    },
                    marker: format!("{} {}", pin.number, pin.name),
                    entity_id: pin.id.clone(),
                });
            }
        }
    }
    for wire in graph.wires.values() {
        if wire.points.len() >= 2 {
            primitives.push(DisplayPrimitive::Polyline {
                points: wire.points.iter().copied().map(point).collect(),
                entity_id: wire.id.clone(),
            });
        }
    }
    for label in graph.labels.values() {
        if inside(label.position, request) {
            primitives.push(DisplayPrimitive::Text {
                position: point(label.position),
                text: label.text.clone(),
                entity_id: label.id.clone(),
            });
        }
    }
    for test_point in graph.test_points.values() {
        if inside(test_point.position, request) {
            primitives.push(DisplayPrimitive::Marker {
                position: point(test_point.position),
                marker: "test-point".to_owned(),
                entity_id: test_point.id.clone(),
            });
        }
    }
    DisplayList {
        revision: request.revision.clone(),
        primitives,
        artifact_refs: Vec::new(),
    }
}

/// Performs deterministic geometric hit testing against semantic primitives.
#[must_use]
pub fn hit_test(graph: &SchematicGraph, request: &HitTestRequest) -> HitTestResult {
    let view_request = ViewRequest {
        revision: request.revision.clone(),
        viewport: jingwei_jdp::Rect {
            min: Point {
                x: request.point.x - request.tolerance,
                y: request.point.y - request.tolerance,
            },
            max: Point {
                x: request.point.x + request.tolerance,
                y: request.point.y + request.tolerance,
            },
        },
        layers: request.layers.clone(),
        level_of_detail: 2,
        selection: Vec::new(),
    };
    let mut hits = display_list(graph, &view_request)
        .primitives
        .iter()
        .filter_map(|primitive| primitive_hit(primitive, request))
        .collect::<Vec<_>>();
    hits.sort_by(|left, right| {
        left.distance
            .partial_cmp(&right.distance)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.entity_id.cmp(&right.entity_id))
    });
    hits.dedup_by(|left, right| left.entity_id == right.entity_id);
    HitTestResult { hits }
}

/// Resolves symbol/part/footprint/BOM/finding relationships by stable ID.
#[must_use]
pub fn cross_probe(
    source_entity: &EntityId,
    index: &CrossProbeIndex,
) -> Option<SchematicCrossProbeResult> {
    let links = index.get(source_entity)?;
    Some(SchematicCrossProbeResult {
        core: CrossProbeResult {
            source_entity: source_entity.clone(),
            related_entities: links.related_entity_ids.clone(),
            relation_kinds: vec!["logical.cross-probe".to_owned(); links.related_entity_ids.len()],
        },
        part_decision_refs: links.part_decision_refs.clone(),
        footprint_asset_refs: links.footprint_asset_refs.clone(),
        bom_line_refs: links.bom_line_refs.clone(),
        finding_refs: links.finding_refs.clone(),
    })
}

fn point(value: PointMm) -> Point {
    Point {
        x: value.x,
        y: value.y,
    }
}

fn inside(value: PointMm, request: &ViewRequest) -> bool {
    value.x >= request.viewport.min.x
        && value.x <= request.viewport.max.x
        && value.y >= request.viewport.min.y
        && value.y <= request.viewport.max.y
}

fn primitive_hit(primitive: &DisplayPrimitive, request: &HitTestRequest) -> Option<Hit> {
    let (entity_id, primitive_kind, distance) = match primitive {
        DisplayPrimitive::Text {
            position,
            entity_id,
            ..
        } => (entity_id, "text", distance(*position, request.point)),
        DisplayPrimitive::Glyph {
            position,
            entity_id,
            ..
        } => (entity_id, "glyph", distance(*position, request.point)),
        DisplayPrimitive::Marker {
            position,
            entity_id,
            ..
        } => (entity_id, "marker", distance(*position, request.point)),
        DisplayPrimitive::Line {
            from,
            to,
            entity_id,
        } => (
            entity_id,
            "line",
            segment_distance(request.point, *from, *to),
        ),
        DisplayPrimitive::Polyline { points, entity_id }
        | DisplayPrimitive::Polygon { points, entity_id } => {
            let distance = points
                .windows(2)
                .map(|window| segment_distance(request.point, window[0], window[1]))
                .fold(f64::INFINITY, f64::min);
            (entity_id, "polyline", distance)
        }
    };
    (distance <= request.tolerance).then(|| Hit {
        entity_id: entity_id.clone(),
        distance,
        primitive_kind: primitive_kind.to_owned(),
    })
}

fn distance(left: Point, right: Point) -> f64 {
    (left.x - right.x).hypot(left.y - right.y)
}

fn segment_distance(point: Point, start: Point, end: Point) -> f64 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    if dx.abs() < f64::EPSILON && dy.abs() < f64::EPSILON {
        return distance(point, start);
    }
    let ratio = ((point.x - start.x) * dx + (point.y - start.y) * dy) / (dx * dx + dy * dy);
    let ratio = ratio.clamp(0.0, 1.0);
    distance(
        point,
        Point {
            x: start.x + ratio * dx,
            y: start.y + ratio * dy,
        },
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use jingwei_jdp::{HitTestRequest, Rect, ViewRequest};
    use jingwei_jir::{EditControl, EntityId, RevisionId};

    use super::*;
    use crate::{Sheet, SymbolInstance};

    fn id(value: &str) -> EntityId {
        EntityId::new(value).expect("valid id")
    }

    fn graph() -> SchematicGraph {
        let sheet_id = id("sheet:root");
        let symbol_id = id("symbol:u1");
        SchematicGraph {
            id: id("schematic:view-test"),
            schema_version: crate::SCHEMATIC_SCHEMA_VERSION.to_owned(),
            sheets: BTreeMap::from([(
                sheet_id.clone(),
                Sheet {
                    id: sheet_id.clone(),
                    parent_sheet_id: None,
                    name: "Root".to_owned(),
                    hierarchical_path: "/".to_owned(),
                },
            )]),
            symbols: BTreeMap::from([(
                symbol_id.clone(),
                SymbolInstance {
                    id: symbol_id,
                    sheet_id,
                    module_id: None,
                    reference: "U1".to_owned(),
                    value: "fixture".to_owned(),
                    exact_mpn: "JW-U1".to_owned(),
                    part_decision_id: "part-decision:u1".to_owned(),
                    symbol_asset_id: "symbol-asset:u1".to_owned(),
                    footprint_asset_id: Some("footprint-asset:u1".to_owned()),
                    position: PointMm { x: 10.0, y: 20.0 },
                    rotation_deg: 0.0,
                    mirrored: false,
                    pins: BTreeMap::new(),
                    properties: BTreeMap::new(),
                    evidence_refs: Vec::new(),
                    edit_control: EditControl::default(),
                },
            )]),
            ..SchematicGraph::default()
        }
    }

    #[test]
    fn display_hit_test_and_cross_probe_keep_stable_identity() {
        let graph = graph();
        let revision = RevisionId::new("revision:view-test").expect("revision");
        let request = ViewRequest {
            revision: revision.clone(),
            viewport: Rect {
                min: Point { x: 0.0, y: 0.0 },
                max: Point { x: 100.0, y: 100.0 },
            },
            layers: vec!["schematic".to_owned()],
            level_of_detail: 1,
            selection: Vec::new(),
        };
        let display = display_list(&graph, &request);
        assert!(!display.primitives.is_empty());
        let result = hit_test(
            &graph,
            &HitTestRequest {
                revision,
                point: Point { x: 10.0, y: 20.0 },
                tolerance: 1.0,
                layers: vec!["schematic".to_owned()],
            },
        );
        assert_eq!(result.hits[0].entity_id, id("symbol:u1"));

        let index = BTreeMap::from([(
            id("symbol:u1"),
            CrossProbeLinks {
                related_entity_ids: vec![id("footprint:u1")],
                part_decision_refs: vec!["part-decision:u1".to_owned()],
                footprint_asset_refs: vec!["footprint-asset:u1".to_owned()],
                bom_line_refs: vec!["bom:U1".to_owned()],
                finding_refs: vec!["finding:u1".to_owned()],
            },
        )]);
        let probed = cross_probe(&id("symbol:u1"), &index).expect("cross probe");
        assert_eq!(probed.core.related_entities, vec![id("footprint:u1")]);
        assert_eq!(probed.bom_line_refs, vec!["bom:U1"]);
    }
}
