//! `KiCad` PCB projection/reference backend.
//!
//! Generated files are structured S-expressions built from an AST.  Existing
//! boards are never modified by regular-expression substitution.

mod reference;
mod sexpr;

pub use reference::{
    KicadReferenceConfig, ReferenceDrcReport, ReferenceRunStatus, run_reference_drc,
};
pub use sexpr::{SExpr, SExprError, parse_sexpr, serialize_sexpr};

use jingwei_geometry::{Length, Point};
use jingwei_pcb::{Board, LayerKind, Side};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Projection implementation version.
pub const KICAD_PROJECTION_VERSION: &str = "jingwei-kicad-pcb-v1";
/// `KiCad` PCB syntax version used by the Phase 0 projection.
pub const KICAD_FILE_FORMAT_VERSION: &str = "20240108";

/// JIR candidate entity to one or more `KiCad` UUIDs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectMapEntry {
    /// Stable Jingwei entity ID.
    pub jir_entity_id: String,
    /// Entity kind.
    pub entity_kind: String,
    /// One or more generated `KiCad` UUIDs.
    pub kicad_uuids: Vec<String>,
}

/// Projection coverage item.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionCoverage {
    /// Capability name.
    pub capability: String,
    /// Whether this projection implements the capability.
    pub covered: bool,
    /// Scope/limitation.
    pub note: String,
}

/// Deterministic `KiCad` projection result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionResult {
    /// Source PCB candidate revision.
    pub candidate_revision: String,
    /// Projection implementation version.
    pub projection_version: String,
    /// UTF-8 `.kicad_pcb` content.
    pub content: String,
    /// Object map sorted by Jingwei ID.
    pub object_map: Vec<ObjectMapEntry>,
    /// Explicit projection coverage.
    pub coverage: Vec<ProjectionCoverage>,
}

/// Projection equivalence result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EquivalenceReport {
    /// Whether every required candidate object has an object-map entry and parseable UUID.
    pub equivalent: bool,
    /// Candidate IDs missing from the map.
    pub missing_jir_objects: Vec<String>,
    /// Object-map UUIDs not present in the parsed projection.
    pub missing_kicad_objects: Vec<String>,
    /// Parse/coverage notes.
    pub notes: Vec<String>,
}

/// Projects a PCB candidate to a typed `KiCad` S-expression.
#[allow(clippy::too_many_lines)]
pub fn project_board(board: &Board) -> ProjectionResult {
    let mut object_map = Vec::new();
    let mut root = vec![
        atom("kicad_pcb"),
        list([atom("version"), atom(KICAD_FILE_FORMAT_VERSION)]),
        list([atom("generator"), atom("jingwei-agent")]),
        list([atom("generator_version"), string(KICAD_PROJECTION_VERSION)]),
        list([
            atom("general"),
            list([
                atom("thickness"),
                atom(&format_mm(board.definition.stackup.board_thickness)),
            ]),
        ]),
        project_layers(board),
        list([atom("setup")]),
    ];

    let mut net_numbers = BTreeMap::new();
    for (index, (net_id, name)) in board.nets.iter().enumerate() {
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        net_numbers.insert(net_id.clone(), number);
        root.push(list([atom("net"), atom(&number.to_string()), string(name)]));
    }

    for edge in board.definition.outline.edges() {
        let uuid = stable_uuid(&format!(
            "board:outline:{}:{}:{}:{}",
            edge.start.x.as_micrometres(),
            edge.start.y.as_micrometres(),
            edge.end.x.as_micrometres(),
            edge.end.y.as_micrometres()
        ));
        root.push(graphic_line(edge.start, edge.end, "Edge.Cuts", &uuid));
        push_map_uuid(&mut object_map, "board:outline", "board-outline", uuid);
    }
    for cutout in &board.definition.cutouts {
        for edge in cutout.edges() {
            let uuid = stable_uuid(&format!(
                "board:cutout:{}:{}:{}:{}",
                edge.start.x.as_micrometres(),
                edge.start.y.as_micrometres(),
                edge.end.x.as_micrometres(),
                edge.end.y.as_micrometres()
            ));
            root.push(graphic_line(edge.start, edge.end, "Edge.Cuts", &uuid));
            push_map_uuid(&mut object_map, "board:cutouts", "board-cutout", uuid);
        }
    }
    for feature in &board.definition.mounting_features {
        let uuid = stable_uuid(&feature.id);
        root.push(list([
            atom("footprint"),
            string("Jingwei:MechanicalHole"),
            list([atom("layer"), string("F.Cu")]),
            list([atom("uuid"), atom(&uuid)]),
            at(feature.center, None),
            list([
                atom("pad"),
                string(""),
                atom("np_thru_hole"),
                atom("circle"),
                list([atom("at"), atom("0"), atom("0")]),
                list([
                    atom("size"),
                    atom(&format_mm(feature.drill)),
                    atom(&format_mm(feature.drill)),
                ]),
                list([atom("drill"), atom(&format_mm(feature.drill))]),
                list([atom("layers"), string("*.Cu"), string("*.Mask")]),
            ]),
        ]));
        push_map_uuid(&mut object_map, &feature.id, "mechanical-feature", uuid);
    }

    for footprint in &board.footprints {
        let Some(position) = footprint.position else {
            continue;
        };
        let uuid = stable_uuid(&footprint.id);
        let layer = if footprint.side == Side::Front {
            "F.Cu"
        } else {
            "B.Cu"
        };
        let mut node = vec![
            atom("footprint"),
            string(&format!("Jingwei:{}", footprint.footprint_name)),
            list([atom("layer"), string(layer)]),
            list([atom("uuid"), atom(&uuid)]),
            at(
                position,
                Some(footprint.rotation.as_microdegrees() / 1_000_000),
            ),
            list([
                atom("property"),
                string("Reference"),
                string(&footprint.designator),
                list([atom("at"), atom("0"), atom("-2")]),
                list([atom("layer"), string("F.SilkS")]),
            ]),
            list([
                atom("property"),
                string("JINGWEI_ID"),
                string(&footprint.id),
                list([atom("at"), atom("0"), atom("0")]),
                list([atom("layer"), string("F.Fab"), atom("hide")]),
            ]),
        ];
        for pad in board
            .pads
            .iter()
            .filter(|pad| pad.footprint_id == footprint.id)
        {
            let pad_uuid = stable_uuid(&pad.id);
            let pad_kind = if pad.drill == Length::ZERO {
                "smd"
            } else {
                "thru_hole"
            };
            let mut pad_node = vec![
                atom("pad"),
                string(&pad.number),
                atom(pad_kind),
                atom(if pad.drill == Length::ZERO {
                    "roundrect"
                } else {
                    "circle"
                }),
                at(pad.offset, None),
                list([
                    atom("size"),
                    atom(&format_mm(pad.size.0)),
                    atom(&format_mm(pad.size.1)),
                ]),
            ];
            if pad.drill != Length::ZERO {
                pad_node.push(list([atom("drill"), atom(&format_mm(pad.drill))]));
            }
            pad_node.push(if pad.drill == Length::ZERO {
                list([
                    atom("layers"),
                    string(if footprint.side == Side::Front {
                        "F.Cu"
                    } else {
                        "B.Cu"
                    }),
                    string(if footprint.side == Side::Front {
                        "F.Paste"
                    } else {
                        "B.Paste"
                    }),
                    string(if footprint.side == Side::Front {
                        "F.Mask"
                    } else {
                        "B.Mask"
                    }),
                ])
            } else {
                list([atom("layers"), string("*.Cu"), string("*.Mask")])
            });
            if pad.drill == Length::ZERO {
                pad_node.push(list([atom("roundrect_rratio"), atom("0.25")]));
            }
            if let Some(net_id) = &pad.net_id
                && let Some(number) = net_numbers.get(net_id)
            {
                pad_node.push(list([
                    atom("net"),
                    atom(&number.to_string()),
                    string(&board.nets[net_id]),
                ]));
            }
            pad_node.push(list([atom("uuid"), atom(&pad_uuid)]));
            node.push(SExpr::List(pad_node));
            push_map_uuid(&mut object_map, &pad.id, "pad", pad_uuid);
        }
        root.push(SExpr::List(node));
        push_map_uuid(&mut object_map, &footprint.id, "footprint", uuid);
    }

    for track in &board.tracks {
        for (index, segment) in track.path.segments().enumerate() {
            let uuid = stable_uuid(&format!("{}:{index}", track.id));
            let mut node = vec![
                atom("segment"),
                point_node("start", segment.start),
                point_node("end", segment.end),
                list([atom("width"), atom(&format_mm(track.width))]),
                list([atom("layer"), string(&track.layer_id)]),
                list([atom("uuid"), atom(&uuid)]),
            ];
            if let Some(number) = net_numbers.get(&track.net_id) {
                node.push(list([atom("net"), atom(&number.to_string())]));
            }
            root.push(SExpr::List(node));
            push_map_uuid(&mut object_map, &track.id, "track", uuid);
        }
    }
    for via in &board.vias {
        let uuid = stable_uuid(&via.id);
        let mut node = vec![
            atom("via"),
            at(via.position, None),
            list([atom("size"), atom(&format_mm(via.diameter))]),
            list([atom("drill"), atom(&format_mm(via.drill))]),
            list([atom("layers"), string("F.Cu"), string("B.Cu")]),
            list([atom("uuid"), atom(&uuid)]),
        ];
        if let Some(number) = net_numbers.get(&via.net_id) {
            node.push(list([atom("net"), atom(&number.to_string())]));
        }
        root.push(SExpr::List(node));
        push_map_uuid(&mut object_map, &via.id, "via", uuid);
    }

    object_map.sort_by(|left, right| left.jir_entity_id.cmp(&right.jir_entity_id));
    let ast = SExpr::List(root);
    ProjectionResult {
        candidate_revision: board.candidate_revision.clone(),
        projection_version: KICAD_PROJECTION_VERSION.to_owned(),
        content: format!("{}\n", serialize_sexpr(&ast)),
        object_map,
        coverage: projection_coverage(),
    }
}

/// Parses and serializes a projection to prove structured round-trip support.
///
/// # Errors
///
/// Returns [`SExprError`] when the input exceeds the parser budget or is not a
/// single well-formed S-expression.
pub fn round_trip_projection(content: &str, maximum_bytes: usize) -> Result<String, SExprError> {
    let ast = parse_sexpr(content, maximum_bytes)?;
    Ok(format!("{}\n", serialize_sexpr(&ast)))
}

/// Checks object-map coverage and UUID presence in a parsed projection.
pub fn check_equivalence(board: &Board, projection: &ProjectionResult) -> EquivalenceReport {
    let mut required = BTreeSet::from(["board:outline".to_owned()]);
    required.extend(
        board
            .definition
            .mounting_features
            .iter()
            .map(|item| item.id.clone()),
    );
    required.extend(
        board
            .footprints
            .iter()
            .filter(|item| item.position.is_some())
            .map(|item| item.id.clone()),
    );
    required.extend(
        board
            .pads
            .iter()
            .filter(|item| item.center.is_some())
            .map(|item| item.id.clone()),
    );
    required.extend(board.tracks.iter().map(|item| item.id.clone()));
    required.extend(board.vias.iter().map(|item| item.id.clone()));
    let mapped: BTreeSet<_> = projection
        .object_map
        .iter()
        .map(|entry| entry.jir_entity_id.clone())
        .collect();
    let missing_jir_objects: Vec<_> = required.difference(&mapped).cloned().collect();
    let parsed = parse_sexpr(&projection.content, 16 * 1024 * 1024);
    let mut missing_kicad_objects = Vec::new();
    let mut notes = Vec::new();
    match parsed {
        Ok(ast) => {
            let atoms = collect_atoms(&ast);
            for uuid in projection
                .object_map
                .iter()
                .flat_map(|entry| entry.kicad_uuids.iter())
            {
                if !atoms.contains(uuid) {
                    missing_kicad_objects.push(uuid.clone());
                }
            }
        }
        Err(error) => notes.push(format!("projection parse failed: {error}")),
    }
    EquivalenceReport {
        equivalent: missing_jir_objects.is_empty()
            && missing_kicad_objects.is_empty()
            && notes.is_empty(),
        missing_jir_objects,
        missing_kicad_objects,
        notes,
    }
}

fn project_layers(board: &Board) -> SExpr {
    let mut layers = vec![atom("layers")];
    let mut copper_index = 0_u16;
    for layer in &board.definition.stackup.layers {
        let kind = match layer.kind {
            LayerKind::Copper => {
                let index = copper_index;
                copper_index = copper_index.saturating_add(2);
                (index, "signal")
            }
            LayerKind::SolderMask => (if layer.name.starts_with('F') { 36 } else { 37 }, "user"),
            LayerKind::Silkscreen => (if layer.name.starts_with('F') { 37 } else { 36 }, "user"),
            LayerKind::EdgeCuts => (44, "user"),
            LayerKind::Fabrication => (49, "user"),
            LayerKind::Courtyard => (39, "user"),
        };
        layers.push(list([
            atom(&kind.0.to_string()),
            string(&layer.name),
            atom(kind.1),
        ]));
    }
    if board.definition.stackup.layers.is_empty() {
        layers.push(list([atom("0"), string("F.Cu"), atom("signal")]));
        layers.push(list([atom("31"), string("B.Cu"), atom("signal")]));
        layers.push(list([atom("36"), string("B.SilkS"), atom("user")]));
        layers.push(list([atom("37"), string("F.SilkS"), atom("user")]));
        layers.push(list([atom("44"), string("Edge.Cuts"), atom("user")]));
    }
    SExpr::List(layers)
}

fn graphic_line(start: Point, end: Point, layer: &str, uuid: &str) -> SExpr {
    list([
        atom("gr_line"),
        point_node("start", start),
        point_node("end", end),
        list([atom("stroke"), list([atom("width"), atom("0.05")])]),
        list([atom("layer"), string(layer)]),
        list([atom("uuid"), atom(uuid)]),
    ])
}

fn at(point: Point, angle_degrees: Option<i64>) -> SExpr {
    let mut values = vec![
        atom("at"),
        atom(&format_length(point.x)),
        atom(&format_length(point.y)),
    ];
    if let Some(angle) = angle_degrees
        && angle != 0
    {
        values.push(atom(&angle.to_string()));
    }
    SExpr::List(values)
}

fn point_node(name: &str, point: Point) -> SExpr {
    list([
        atom(name),
        atom(&format_length(point.x)),
        atom(&format_length(point.y)),
    ])
}

fn format_length(length: Length) -> String {
    format_mm(length)
}

fn format_mm(length: Length) -> String {
    let value = length.as_micrometres();
    let sign = if value < 0 { "-" } else { "" };
    let absolute = value.unsigned_abs();
    let whole = absolute / 1_000;
    let fraction = absolute % 1_000;
    if fraction == 0 {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{fraction:03}")
            .trim_end_matches('0')
            .to_owned()
    }
}

fn stable_uuid(id: &str) -> String {
    let bytes = Sha256::digest(id.as_bytes());
    let hex = hex::encode(&bytes[..16]);
    format!(
        "{}-{}-4{}-a{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        &hex[17..20],
        &hex[20..32]
    )
}

fn push_map_uuid(map: &mut Vec<ObjectMapEntry>, id: &str, kind: &str, uuid: String) {
    if let Some(existing) = map.iter_mut().find(|entry| entry.jir_entity_id == id) {
        existing.kicad_uuids.push(uuid);
    } else {
        map.push(ObjectMapEntry {
            jir_entity_id: id.to_owned(),
            entity_kind: kind.to_owned(),
            kicad_uuids: vec![uuid],
        });
    }
}

fn collect_atoms(expression: &SExpr) -> BTreeSet<String> {
    fn visit(expression: &SExpr, result: &mut BTreeSet<String>) {
        match expression {
            SExpr::Atom(value) | SExpr::String(value) => {
                result.insert(value.clone());
            }
            SExpr::List(values) => {
                for value in values {
                    visit(value, result);
                }
            }
        }
    }
    let mut result = BTreeSet::new();
    visit(expression, &mut result);
    result
}

fn projection_coverage() -> Vec<ProjectionCoverage> {
    vec![
        ProjectionCoverage {
            capability: "outline-cutout-footprint-pad-track-via".to_owned(),
            covered: true,
            note: "typed AST projection and external object map".to_owned(),
        },
        ProjectionCoverage {
            capability: "zone-fill".to_owned(),
            covered: false,
            note: "zone fill remains a native/reference backend follow-up".to_owned(),
        },
        ProjectionCoverage {
            capability: "differential-length-impedance-rules".to_owned(),
            covered: false,
            note: "model fields are retained in JIR candidate but not projected in Phase 0"
                .to_owned(),
        },
        ProjectionCoverage {
            capability: "arbitrary-third-party-round-trip".to_owned(),
            covered: false,
            note: "parser round-trips supported syntax; unknown destructive import is rejected"
                .to_owned(),
        },
    ]
}

fn atom(value: &str) -> SExpr {
    SExpr::Atom(value.to_owned())
}

fn string(value: &str) -> SExpr {
    SExpr::String(value.to_owned())
}

fn list(values: impl IntoIterator<Item = SExpr>) -> SExpr {
    SExpr::List(values.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jingwei_geometry::{Point, Polygon};
    use jingwei_pcb::{BoardDefinition, PcbRules, Stackup};

    fn board() -> Board {
        Board {
            schema_version: "pcb-candidate-v0.1".to_owned(),
            source_revision: "logical".to_owned(),
            candidate_revision: "pcb".to_owned(),
            definition: BoardDefinition {
                outline: Polygon::rectangle(
                    Point::from_millimetres(0, 0),
                    Point::from_millimetres(60, 40),
                ),
                cutouts: vec![],
                mounting_features: vec![],
                stackup: Stackup {
                    board_thickness: Length::from_micrometres(1_600),
                    layers: vec![],
                    surface_finish: "HASL".to_owned(),
                },
                regions: vec![],
                origin: Point::from_millimetres(0, 0),
            },
            rules: PcbRules::default(),
            footprints: vec![],
            pads: vec![],
            nets: BTreeMap::new(),
            net_priorities: BTreeMap::new(),
            net_classes: vec![],
            tracks: vec![],
            vias: vec![],
            zones: vec![],
            keepouts: vec![],
            texts: vec![],
            pin_pad_map: BTreeMap::new(),
            evidence_refs: vec![],
        }
    }

    #[test]
    fn projection_is_parseable_and_round_trips_structurally() {
        let projection = project_board(&board());
        let round_trip = round_trip_projection(&projection.content, 1024 * 1024).unwrap();
        assert_eq!(round_trip, projection.content);
        assert!(check_equivalence(&board(), &projection).equivalent);
        assert!(projection.content.contains("jingwei-agent"));
    }

    #[test]
    fn missing_object_map_entry_blocks_equivalence() {
        let mut projection = project_board(&board());
        projection.object_map.clear();
        let report = check_equivalence(&board(), &projection);
        assert!(!report.equivalent);
        assert_eq!(report.missing_jir_objects, ["board:outline"]);
    }

    #[test]
    fn oversized_projection_is_rejected_before_parsing() {
        let error = parse_sexpr("(x)", 2).unwrap_err();
        assert!(matches!(error, SExprError::InputTooLarge { .. }));
    }
}
