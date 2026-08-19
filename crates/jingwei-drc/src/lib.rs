//! Native PCB and manufacturing checks.
//!
//! The report is always bound to the exact candidate revision, board hash,
//! profile and rule-set version.  Zero native errors does not cover SI, PI,
//! thermal, EMC or manufacturer DFM; those gaps are returned explicitly.

use jingwei_geometry::{BoundingBox, Length, Point};
use jingwei_pcb::{Board, EditControl, Pad, Track, Via};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Native PCB rule-set version.
pub const NATIVE_DRC_VERSION: &str = "jingwei-native-pcb-drc-v1";

/// Finding severity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// Blocks candidate eligibility.
    Error,
    /// Requires repair or a named waiver in an integration workflow.
    Warning,
    /// Coverage or review information.
    Info,
}

/// Machine-readable DRC/DFM finding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable finding ID within this report.
    pub id: String,
    /// Rule ID.
    pub rule_id: String,
    /// Rule implementation version.
    pub rule_version: String,
    /// Severity.
    pub severity: Severity,
    /// Short title.
    pub title: String,
    /// Affected object IDs.
    pub object_refs: Vec<String>,
    /// Board location when applicable.
    pub location: Option<Point>,
    /// Manufacturer profile ID.
    pub profile_id: String,
    /// Evidence/profile source references.
    pub evidence_refs: Vec<String>,
    /// Suggested deterministic repair direction.
    pub suggested_fix: String,
}

/// Coverage maturity for a validation area.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoverageStatus {
    /// Deterministic Phase 0 rule implemented.
    Implemented,
    /// A conservative heuristic is implemented and needs engineering review.
    Heuristic,
    /// Not implemented and cannot be inferred from a zero-error report.
    Gap,
}

/// One validation coverage item.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageItem {
    /// Coverage area ID.
    pub capability: String,
    /// Coverage maturity.
    pub status: CoverageStatus,
    /// Scope and limitation note.
    pub note: String,
}

/// Revision-bound native DRC report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrcReport {
    /// Candidate revision.
    pub candidate_revision: String,
    /// Exact serialized board hash.
    pub board_hash: String,
    /// Native rule-set version.
    pub rule_set_version: String,
    /// Manufacturer profile/rule version.
    pub profile_rule_version: String,
    /// Sorted findings.
    pub findings: Vec<Finding>,
    /// Explicit coverage map.
    pub coverage: Vec<CoverageItem>,
}

impl DrcReport {
    /// Number of error findings.
    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Error)
            .count()
    }

    /// Number of warning findings.
    pub fn warning_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Warning)
            .count()
    }
}

/// Runs all Phase 0 native checks and returns findings even when some rules fail.
pub fn run_native_drc(board: &Board) -> DrcReport {
    let mut findings = Vec::new();
    check_copper_clearance(board, &mut findings);
    check_track_width(board, &mut findings);
    check_vias(board, &mut findings);
    check_connectivity(board, &mut findings);
    check_board_edge(board, &mut findings);
    check_courtyard_overlap(board, &mut findings);
    check_holes_and_slots(board, &mut findings);
    check_solder_mask_bridge(board, &mut findings);
    check_silkscreen(board, &mut findings);
    check_keepouts(board, &mut findings);
    check_locked_positions(board, &mut findings);
    check_testpoint_coverage(board, &mut findings);
    findings.sort_by(|left, right| {
        (left.severity, left.rule_id.as_str(), left.id.as_str()).cmp(&(
            right.severity,
            right.rule_id.as_str(),
            right.id.as_str(),
        ))
    });
    DrcReport {
        candidate_revision: board.candidate_revision.clone(),
        board_hash: board.deterministic_hash(),
        rule_set_version: NATIVE_DRC_VERSION.to_owned(),
        profile_rule_version: board.rules.version.clone(),
        findings,
        coverage: coverage_map(),
    }
}

fn check_copper_clearance(board: &Board, findings: &mut Vec<Finding>) {
    let clearance = board.rules.minimum_clearance;
    let required_squared = i128::from(clearance.as_micrometres()).pow(2);
    for left in 0..board.tracks.len() {
        for right in (left + 1)..board.tracks.len() {
            let a = &board.tracks[left];
            let b = &board.tracks[right];
            if a.net_id != b.net_id
                && track_bounds(a).squared_distance(track_bounds(b)) < required_squared
            {
                findings.push(finding(
                    board,
                    "pcb.copper-clearance",
                    Severity::Error,
                    "Copper clearance below profile minimum",
                    vec![a.id.clone(), b.id.clone()],
                    a.path.points.first().copied(),
                    "Move or reroute one copper object to satisfy the active net-class clearance.",
                ));
            }
        }
    }
}

fn check_track_width(board: &Board, findings: &mut Vec<Finding>) {
    for track in &board.tracks {
        if track.width < board.rules.minimum_track_width {
            findings.push(finding(
                board,
                "pcb.track-width",
                Severity::Error,
                "Track width below profile minimum",
                vec![track.id.clone()],
                track.path.points.first().copied(),
                "Use the active net-class width or a wider profile-approved width.",
            ));
        }
    }
}

fn check_vias(board: &Board, findings: &mut Vec<Finding>) {
    for via in &board.vias {
        let annular_ring = (via.diameter.as_micrometres() - via.drill.as_micrometres()) / 2;
        if via.drill < board.rules.minimum_drill
            || annular_ring < board.rules.minimum_annular_ring.as_micrometres()
            || via.diameter <= via.drill
        {
            findings.push(finding(
                board,
                "pcb.via-drill-annular-ring",
                Severity::Error,
                "Via drill or annular ring violates the profile",
                vec![via.id.clone()],
                Some(via.position),
                "Select a via class with a compliant drill and annular ring.",
            ));
        }
    }
}

fn check_connectivity(board: &Board, findings: &mut Vec<Finding>) {
    for net_id in board.nets.keys() {
        let pads = board.pad_centers_for_net(net_id);
        if pads.len() < 2 {
            continue;
        }
        let tracks: Vec<_> = board
            .tracks
            .iter()
            .filter(|track| track.net_id == *net_id)
            .collect();
        if !pads_connected(&pads, &tracks) {
            findings.push(finding(
                board,
                "pcb.unconnected-net",
                Severity::Error,
                "Net has unconnected or unrouted pads",
                pads.iter().map(|(id, _)| id.clone()).collect(),
                pads.first().map(|(_, point)| *point),
                "Route every pad in the net and rerun native connectivity validation.",
            ));
        }
    }
}

fn check_board_edge(board: &Board, findings: &mut Vec<Finding>) {
    let margin = board.rules.minimum_edge_clearance;
    for pad in &board.pads {
        if let Some(bounds) = pad_bounds(pad) {
            let expanded = bounds.expanded(margin);
            if !board.definition.outline.contains(expanded.min)
                || !board.definition.outline.contains(expanded.max)
            {
                findings.push(finding(
                    board,
                    "pcb.copper-to-edge",
                    Severity::Error,
                    "Pad is too close to the board edge",
                    vec![pad.id.clone()],
                    pad.center,
                    "Move the footprint inward or use a reviewed edge-connector exception.",
                ));
            }
        }
    }
    for track in &board.tracks {
        let expanded = track_bounds(track).expanded(margin);
        if !board.definition.outline.contains(expanded.min)
            || !board.definition.outline.contains(expanded.max)
        {
            findings.push(finding(
                board,
                "pcb.copper-to-edge",
                Severity::Error,
                "Track is too close to the board edge",
                vec![track.id.clone()],
                track.path.points.first().copied(),
                "Reroute copper inside the profile edge-clearance boundary.",
            ));
        }
    }
    for via in &board.vias {
        let bounds = via_bounds(via).expanded(margin);
        if !board.definition.outline.contains(bounds.min)
            || !board.definition.outline.contains(bounds.max)
        {
            findings.push(finding(
                board,
                "pcb.copper-to-edge",
                Severity::Error,
                "Via is too close to the board edge",
                vec![via.id.clone()],
                Some(via.position),
                "Move the via inside the profile edge-clearance boundary.",
            ));
        }
    }
}

fn check_courtyard_overlap(board: &Board, findings: &mut Vec<Finding>) {
    for left in 0..board.footprints.len() {
        for right in (left + 1)..board.footprints.len() {
            let a = &board.footprints[left];
            let b = &board.footprints[right];
            if a.courtyard_bounds()
                .zip(b.courtyard_bounds())
                .is_some_and(|(a_bounds, b_bounds)| a_bounds.intersects(b_bounds))
            {
                findings.push(finding(
                    board,
                    "pcb.courtyard-overlap",
                    Severity::Error,
                    "Footprint courtyards overlap",
                    vec![a.id.clone(), b.id.clone()],
                    a.position,
                    "Move one footprint while preserving locked mechanical objects.",
                ));
            }
        }
    }
}

fn check_holes_and_slots(board: &Board, findings: &mut Vec<Finding>) {
    for feature in &board.definition.mounting_features {
        let invalid_drill = feature.drill < board.rules.minimum_npth;
        let invalid_slot =
            feature.slot_length != Length::ZERO && feature.slot_length < feature.drill;
        if invalid_drill || invalid_slot {
            findings.push(finding(
                board,
                "pcb.hole-slot",
                Severity::Error,
                "Hole or slot violates the profile",
                vec![feature.id.clone()],
                Some(feature.center),
                "Increase the hole/slot geometry or request a reviewed manufacturer capability.",
            ));
        }
    }
}

fn check_solder_mask_bridge(board: &Board, findings: &mut Vec<Finding>) {
    let required = board.rules.minimum_mask_bridge;
    let required_squared = i128::from(required.as_micrometres()).pow(2);
    for left in 0..board.pads.len() {
        for right in (left + 1)..board.pads.len() {
            let a = &board.pads[left];
            let b = &board.pads[right];
            let Some((a_bounds, b_bounds)) = pad_bounds(a).zip(pad_bounds(b)) else {
                continue;
            };
            let a_mask = a_bounds.expanded(a.mask_expansion);
            let b_mask = b_bounds.expanded(b.mask_expansion);
            if a_mask.squared_distance(b_mask) < required_squared {
                findings.push(finding(
                    board,
                    "pcb.solder-mask-bridge",
                    Severity::Error,
                    "Solder-mask bridge is below the profile minimum",
                    vec![a.id.clone(), b.id.clone()],
                    a.center,
                    "Increase pad spacing or use a reviewed solder-mask opening strategy.",
                ));
            }
        }
    }
}

fn check_silkscreen(board: &Board, findings: &mut Vec<Finding>) {
    for text in &board.texts {
        let outside = !board.definition.outline.contains(text.bounds.min)
            || !board.definition.outline.contains(text.bounds.max);
        let over_pad = board
            .pads
            .iter()
            .filter_map(pad_bounds)
            .any(|pad_bounds| pad_bounds.intersects(text.bounds));
        if outside || over_pad {
            findings.push(finding(
                board,
                "pcb.silkscreen-placement",
                Severity::Error,
                "Silkscreen is outside the board or over a pad",
                vec![text.id.clone()],
                Some(text.position),
                "Move or shorten the silkscreen text so it is printable and readable.",
            ));
        }
    }
}

fn check_keepouts(board: &Board, findings: &mut Vec<Finding>) {
    for keepout in &board.keepouts {
        let bounds = keepout.boundary.bounds();
        if keepout.copper {
            for track in &board.tracks {
                if bounds.intersects(track_bounds(track)) {
                    findings.push(finding(
                        board,
                        "pcb.keepout-violation",
                        Severity::Error,
                        "Track intersects a copper keepout",
                        vec![keepout.id.clone(), track.id.clone()],
                        track.path.points.first().copied(),
                        "Reroute the track outside the keepout.",
                    ));
                }
            }
            for via in &board.vias {
                if keepout.boundary.contains(via.position) {
                    findings.push(finding(
                        board,
                        "pcb.keepout-violation",
                        Severity::Error,
                        "Via is inside a copper keepout",
                        vec![keepout.id.clone(), via.id.clone()],
                        Some(via.position),
                        "Move the via outside the keepout.",
                    ));
                }
            }
        }
        if keepout.placement {
            for footprint in &board.footprints {
                if footprint
                    .courtyard_bounds()
                    .is_some_and(|footprint_bounds| bounds.intersects(footprint_bounds))
                {
                    findings.push(finding(
                        board,
                        "pcb.keepout-violation",
                        Severity::Error,
                        "Footprint intersects a placement keepout",
                        vec![keepout.id.clone(), footprint.id.clone()],
                        footprint.position,
                        "Move the footprint outside the placement keepout.",
                    ));
                }
            }
        }
    }
}

fn check_locked_positions(board: &Board, findings: &mut Vec<Finding>) {
    for footprint in &board.footprints {
        if footprint.edit_control == EditControl::Locked
            && footprint.locked_position.is_some()
            && footprint.position != footprint.locked_position
        {
            findings.push(finding(
                board,
                "pcb.locked-position",
                Severity::Error,
                "Locked footprint moved from its approved position",
                vec![footprint.id.clone()],
                footprint.position,
                "Restore the locked position or use an explicit unlock/review transaction.",
            ));
        }
    }
}

fn check_testpoint_coverage(board: &Board, findings: &mut Vec<Finding>) {
    let testpoint_nets: BTreeSet<_> = board
        .footprints
        .iter()
        .filter(|footprint| footprint.designator.starts_with("TP"))
        .flat_map(|footprint| {
            board
                .pads
                .iter()
                .filter(move |pad| pad.footprint_id == footprint.id)
                .filter_map(|pad| pad.net_id.clone())
        })
        .collect();
    for (net_id, priority) in &board.net_priorities {
        if *priority <= 10 && !testpoint_nets.contains(net_id) {
            findings.push(finding(
                board,
                "pcb.testpoint-coverage",
                Severity::Warning,
                "Critical net lacks an identified test point",
                vec![net_id.clone()],
                None,
                "Add a profile-compliant test point or obtain a named testability review.",
            ));
        }
    }
}

fn pads_connected(pads: &[(String, Point)], tracks: &[&Track]) -> bool {
    if tracks.is_empty() {
        return false;
    }
    let mut adjacency: BTreeMap<PointKey, BTreeSet<PointKey>> = BTreeMap::new();
    for track in tracks {
        let Some(start) = track.path.points.first().copied() else {
            continue;
        };
        let Some(end) = track.path.points.last().copied() else {
            continue;
        };
        let start = PointKey::from(start);
        let end = PointKey::from(end);
        adjacency.entry(start).or_default().insert(end);
        adjacency.entry(end).or_default().insert(start);
    }
    let start = PointKey::from(pads[0].1);
    if !adjacency.contains_key(&start) {
        return false;
    }
    let mut visited = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(point) = queue.pop_front() {
        for next in adjacency.get(&point).into_iter().flatten() {
            if visited.insert(*next) {
                queue.push_back(*next);
            }
        }
    }
    pads.iter()
        .all(|(_, point)| visited.contains(&PointKey::from(*point)))
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PointKey(i64, i64);

impl From<Point> for PointKey {
    fn from(point: Point) -> Self {
        Self(point.x.as_micrometres(), point.y.as_micrometres())
    }
}

fn track_bounds(track: &Track) -> BoundingBox {
    track
        .path
        .bounds()
        .expanded(Length::from_micrometres(track.width.as_micrometres() / 2))
}

fn via_bounds(via: &Via) -> BoundingBox {
    let radius = Length::from_micrometres(via.diameter.as_micrometres() / 2);
    BoundingBox::new(
        Point {
            x: via.position.x - radius,
            y: via.position.y - radius,
        },
        Point {
            x: via.position.x + radius,
            y: via.position.y + radius,
        },
    )
}

fn pad_bounds(pad: &Pad) -> Option<BoundingBox> {
    let center = pad.center?;
    let half_width = Length::from_micrometres(pad.size.0.as_micrometres() / 2);
    let half_height = Length::from_micrometres(pad.size.1.as_micrometres() / 2);
    Some(BoundingBox::new(
        Point {
            x: center.x - half_width,
            y: center.y - half_height,
        },
        Point {
            x: center.x + half_width,
            y: center.y + half_height,
        },
    ))
}

fn finding(
    board: &Board,
    rule_id: &str,
    severity: Severity,
    title: &str,
    mut object_refs: Vec<String>,
    location: Option<Point>,
    suggested_fix: &str,
) -> Finding {
    object_refs.sort();
    let suffix = object_refs.join("+");
    Finding {
        id: format!("finding:{rule_id}:{suffix}"),
        rule_id: rule_id.to_owned(),
        rule_version: NATIVE_DRC_VERSION.to_owned(),
        severity,
        title: title.to_owned(),
        object_refs,
        location,
        profile_id: board.rules.profile_id.clone(),
        evidence_refs: board.rules.evidence_refs.clone(),
        suggested_fix: suggested_fix.to_owned(),
    }
}

fn coverage_map() -> Vec<CoverageItem> {
    let implemented = [
        "copper-clearance",
        "track-width",
        "via-drill-annular-ring",
        "unconnected-net",
        "copper-to-edge",
        "courtyard-overlap",
        "hole-slot",
        "solder-mask-bridge",
        "silkscreen-placement",
        "keepout-region",
        "locked-position",
    ];
    let mut coverage: Vec<_> = implemented
        .into_iter()
        .map(|capability| CoverageItem {
            capability: capability.to_owned(),
            status: CoverageStatus::Implemented,
            note: "deterministic Phase 0 native rule".to_owned(),
        })
        .collect();
    coverage.push(CoverageItem {
        capability: "testpoint-coverage".to_owned(),
        status: CoverageStatus::Heuristic,
        note: "designator/net-priority heuristic; requires test engineering review".to_owned(),
    });
    for capability in [
        "signal-integrity",
        "power-integrity",
        "thermal-simulation",
        "emc",
        "controlled-impedance",
        "high-speed-return-path",
    ] {
        coverage.push(CoverageItem {
            capability: capability.to_owned(),
            status: CoverageStatus::Gap,
            note: "not implemented; zero native errors does not validate this capability"
                .to_owned(),
        });
    }
    coverage
}

#[cfg(test)]
mod tests {
    use super::*;
    use jingwei_geometry::{Angle, Polygon, Polyline};
    use jingwei_pcb::{
        BoardDefinition, Footprint, Keepout, MechanicalFeature, PcbRules, Side, Stackup, TextItem,
    };

    fn base_board() -> Board {
        Board {
            schema_version: "pcb-candidate-v0.1".to_owned(),
            source_revision: "logical".to_owned(),
            candidate_revision: "pcb-invalid".to_owned(),
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

    fn footprint(id: &str, position: Point, locked_position: Option<Point>) -> Footprint {
        Footprint {
            id: id.to_owned(),
            logical_component_id: format!("logical:{id}"),
            designator: id.to_owned(),
            value: "fixture".to_owned(),
            manufacturer: "fixture".to_owned(),
            mpn: format!("fixture-{id}"),
            footprint_name: "fixture".to_owned(),
            placement_role: "ordinary".to_owned(),
            position: Some(position),
            rotation: Angle::ZERO,
            side: Side::Front,
            body_size: (Length::from_millimetres(4), Length::from_millimetres(4)),
            courtyard_margin: Length::from_micrometres(250),
            height: Length::from_millimetres(1),
            edit_control: if locked_position.is_some() {
                EditControl::Locked
            } else {
                EditControl::AiEditable
            },
            locked_position,
        }
    }

    #[test]
    fn clean_empty_board_has_zero_errors_but_explicit_gaps() {
        let report = run_native_drc(&base_board());
        assert_eq!(report.error_count(), 0);
        assert!(
            report
                .coverage
                .iter()
                .any(|item| item.capability == "emc" && item.status == CoverageStatus::Gap)
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn deliberately_broken_board_triggers_ten_rule_classes() {
        let mut board = base_board();
        board.nets = BTreeMap::from([
            ("net:a".to_owned(), "A".to_owned()),
            ("net:b".to_owned(), "B".to_owned()),
        ]);
        board.net_priorities = BTreeMap::from([("net:a".to_owned(), 1)]);
        board.footprints = vec![
            footprint(
                "U1",
                Point::from_millimetres(10, 10),
                Some(Point::from_millimetres(20, 20)),
            ),
            footprint("U2", Point::from_millimetres(10, 10), None),
        ];
        board.pads = vec![
            Pad {
                id: "pad:a1".to_owned(),
                footprint_id: "U1".to_owned(),
                number: "1".to_owned(),
                offset: Point::default(),
                center: Some(Point::from_micrometres(100, 100)),
                size: (Length::from_millimetres(1), Length::from_millimetres(1)),
                drill: Length::ZERO,
                plated: false,
                net_id: Some("net:a".to_owned()),
                mask_expansion: Length::from_micrometres(100),
            },
            Pad {
                id: "pad:a2".to_owned(),
                footprint_id: "U2".to_owned(),
                number: "1".to_owned(),
                offset: Point::default(),
                center: Some(Point::from_micrometres(1_050, 100)),
                size: (Length::from_millimetres(1), Length::from_millimetres(1)),
                drill: Length::ZERO,
                plated: false,
                net_id: Some("net:a".to_owned()),
                mask_expansion: Length::from_micrometres(100),
            },
        ];
        board.tracks = vec![
            Track {
                id: "track:a".to_owned(),
                net_id: "net:a".to_owned(),
                layer_id: "F.Cu".to_owned(),
                width: Length::from_micrometres(100),
                path: Polyline::new(vec![
                    Point::from_millimetres(20, 20),
                    Point::from_millimetres(30, 20),
                ])
                .unwrap(),
                provenance: "fixture".to_owned(),
                edit_control: EditControl::AiEditable,
            },
            Track {
                id: "track:b".to_owned(),
                net_id: "net:b".to_owned(),
                layer_id: "F.Cu".to_owned(),
                width: Length::from_micrometres(200),
                path: Polyline::new(vec![
                    Point::from_micrometres(20_000, 20_100),
                    Point::from_micrometres(30_000, 20_100),
                ])
                .unwrap(),
                provenance: "fixture".to_owned(),
                edit_control: EditControl::AiEditable,
            },
        ];
        board.vias.push(Via {
            id: "via:bad".to_owned(),
            net_id: "net:a".to_owned(),
            position: Point::from_millimetres(25, 20),
            diameter: Length::from_micrometres(350),
            drill: Length::from_micrometres(250),
            edit_control: EditControl::AiEditable,
        });
        board.definition.mounting_features.push(MechanicalFeature {
            id: "hole:bad".to_owned(),
            center: Point::from_millimetres(5, 5),
            drill: Length::from_micrometres(200),
            slot_length: Length::from_micrometres(100),
            plated: false,
            locked: true,
        });
        board.texts.push(TextItem {
            id: "text:bad".to_owned(),
            layer_id: "F.SilkS".to_owned(),
            position: Point::from_micrometres(100, 100),
            text: "BAD".to_owned(),
            bounds: BoundingBox::new(
                Point::from_micrometres(-100, -100),
                Point::from_micrometres(500, 500),
            ),
        });
        board.keepouts.push(Keepout {
            id: "keepout:bad".to_owned(),
            boundary: Polygon::rectangle(
                Point::from_millimetres(19, 19),
                Point::from_millimetres(31, 21),
            ),
            copper: true,
            placement: false,
        });
        let report = run_native_drc(&board);
        let rules: BTreeSet<_> = report
            .findings
            .iter()
            .map(|finding| finding.rule_id.as_str())
            .collect();
        for expected in [
            "pcb.copper-clearance",
            "pcb.track-width",
            "pcb.via-drill-annular-ring",
            "pcb.unconnected-net",
            "pcb.copper-to-edge",
            "pcb.courtyard-overlap",
            "pcb.hole-slot",
            "pcb.solder-mask-bridge",
            "pcb.silkscreen-placement",
            "pcb.keepout-violation",
            "pcb.locked-position",
        ] {
            assert!(rules.contains(expected), "missing {expected}: {rules:?}");
        }
    }
}
