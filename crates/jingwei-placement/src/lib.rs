//! Deterministic Phase 0 placement candidate generation.
//!
//! The implementation is intentionally a baseline heuristic, not a commercial
//! global optimizer.  It fixes mechanical/locked objects first and records a
//! reproducible seed, algorithm version, input hash and metric breakdown.

use jingwei_geometry::{BoundingBox, Length, Point};
use jingwei_pcb::{Board, EditControl};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Baseline placement algorithm version recorded in every candidate.
pub const BASELINE_PLACER_VERSION: &str = "baseline-grid-cluster-v1";

/// Request for a full or local placement candidate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementRequest {
    /// Deterministic seed.
    pub seed: u64,
    /// Optional footprint selection; empty means all editable footprints.
    pub selected_footprint_ids: Vec<String>,
    /// Optional region limiting newly placed centers.
    pub region: Option<BoundingBox>,
    /// Grid spacing.
    pub grid_step: Length,
}

impl Default for PlacementRequest {
    fn default() -> Self {
        Self {
            seed: 0,
            selected_footprint_ids: vec![],
            region: None,
            grid_step: Length::from_millimetres(5),
        }
    }
}

/// Explainable placement metrics. Values are integer Phase 0 proxies.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementMetrics {
    /// Sum of Manhattan pad-to-pad spans in micrometres.
    pub ratsnest_micrometres: i64,
    /// Overlapping courtyard pairs.
    pub congestion_pairs: u64,
    /// Critical-net loop proxy.
    pub loop_area_proxy: i128,
    /// Component-height clustering proxy.
    pub thermal_proxy: i64,
    /// Assembly-direction penalty.
    pub assembly_penalty: u64,
    /// Named integer cost terms.
    pub cost_breakdown: BTreeMap<String, i128>,
}

/// Candidate-only placement result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementCandidate {
    /// Base candidate revision.
    pub base_candidate_revision: String,
    /// Algorithm version.
    pub algorithm_version: String,
    /// Input board hash.
    pub input_hash: String,
    /// Deterministic seed.
    pub seed: u64,
    /// Isolated result board.
    pub board: Board,
    /// Hard-constraint violations; a non-empty list makes the candidate ineligible.
    pub hard_violations: Vec<String>,
    /// Explainable metrics.
    pub metrics: PlacementMetrics,
}

impl PlacementCandidate {
    /// Returns whether the candidate may be ranked by soft score.
    pub fn eligible(&self) -> bool {
        self.hard_violations.is_empty()
    }
}

/// Generates a deterministic grid/cluster placement candidate.
pub fn place_baseline(board: &Board, request: &PlacementRequest) -> PlacementCandidate {
    let mut result = board.clone();
    let input_hash = board.deterministic_hash();
    let selected: BTreeSet<_> = request.selected_footprint_ids.iter().cloned().collect();
    let outline_bounds = board.definition.outline.bounds();
    let region = request.region.unwrap_or(outline_bounds);
    let step = request.grid_step.as_micrometres().max(1);
    let seed_offset_x = i64::try_from(request.seed % 3).unwrap_or(0) * step;
    let seed_offset_y = i64::try_from((request.seed / 3) % 3).unwrap_or(0) * step;
    let start_x = region.min.x.as_micrometres() + step + seed_offset_x;
    let start_y = region.min.y.as_micrometres() + step + seed_offset_y;
    let end_x = region.max.x.as_micrometres() - step;
    let end_y = region.max.y.as_micrometres() - step;
    let mut grid = Vec::new();
    let mut y = start_y;
    while y <= end_y {
        let mut x = start_x;
        while x <= end_x {
            grid.push(Point::from_micrometres(x, y));
            x += step;
        }
        y += step;
    }

    let role_order = |role: &str| match role {
        "power" => 0,
        "critical" => 1,
        "interface" => 2,
        "connector" => 3,
        _ => 4,
    };
    let mut indices: Vec<_> = result
        .footprints
        .iter()
        .enumerate()
        .filter(|(_, footprint)| {
            footprint.edit_control != EditControl::Locked
                && (selected.is_empty() || selected.contains(&footprint.id))
        })
        .map(|(index, footprint)| {
            (
                role_order(&footprint.placement_role),
                footprint.id.clone(),
                index,
            )
        })
        .collect();
    indices.sort();

    let mut hard_violations = Vec::new();
    let mut cursor = 0_usize;
    for (_, id, index) in indices {
        let mut placed = false;
        while cursor < grid.len() {
            let candidate = grid[cursor];
            cursor += 1;
            result.footprints[index].position = Some(candidate);
            if footprint_hard_valid(&result, index, region) {
                placed = true;
                break;
            }
        }
        if !placed {
            result.footprints[index].position = None;
            hard_violations.push(format!("no legal baseline position for {id}"));
        }
    }
    result.rebuild_pad_centers();

    for footprint in &result.footprints {
        if footprint.edit_control == EditControl::Locked
            && footprint.position != footprint.locked_position
        {
            hard_violations.push(format!("locked position changed for {}", footprint.id));
        }
        if footprint.position.is_none() {
            hard_violations.push(format!("unplaced footprint {}", footprint.id));
        }
    }
    hard_violations.sort();
    hard_violations.dedup();
    let metrics = measure(&result);
    result.candidate_revision = format!(
        "{}:placement:{}:{:016x}",
        board.candidate_revision, BASELINE_PLACER_VERSION, request.seed
    );
    PlacementCandidate {
        base_candidate_revision: board.candidate_revision.clone(),
        algorithm_version: BASELINE_PLACER_VERSION.to_owned(),
        input_hash,
        seed: request.seed,
        board: result,
        hard_violations,
        metrics,
    }
}

fn footprint_hard_valid(board: &Board, index: usize, region: BoundingBox) -> bool {
    let footprint = &board.footprints[index];
    let Some(position) = footprint.position else {
        return false;
    };
    let Some(courtyard) = footprint.courtyard_bounds() else {
        return false;
    };
    if !region.contains(position)
        || !board.definition.outline.contains(courtyard.min)
        || !board.definition.outline.contains(courtyard.max)
    {
        return false;
    }
    if board.keepouts.iter().any(|keepout| {
        keepout.placement
            && (keepout.boundary.contains(courtyard.min)
                || keepout.boundary.contains(courtyard.max)
                || keepout.boundary.contains(position))
    }) {
        return false;
    }
    !board
        .footprints
        .iter()
        .enumerate()
        .any(|(other_index, other)| {
            other_index != index
                && other.position.is_some()
                && other
                    .courtyard_bounds()
                    .is_some_and(|bounds| bounds.intersects(courtyard))
        })
}

fn measure(board: &Board) -> PlacementMetrics {
    let mut ratsnest = 0_i64;
    let mut loop_proxy = 0_i128;
    for (net_id, priority) in &board.net_priorities {
        let centers = board.pad_centers_for_net(net_id);
        if centers.len() >= 2 {
            let xs: Vec<_> = centers
                .iter()
                .map(|(_, point)| point.x.as_micrometres())
                .collect();
            let ys: Vec<_> = centers
                .iter()
                .map(|(_, point)| point.y.as_micrometres())
                .collect();
            let span_x = xs.iter().max().unwrap_or(&0) - xs.iter().min().unwrap_or(&0);
            let span_y = ys.iter().max().unwrap_or(&0) - ys.iter().min().unwrap_or(&0);
            ratsnest = ratsnest.saturating_add(span_x.saturating_add(span_y));
            if *priority <= 10 {
                loop_proxy += i128::from(span_x) * i128::from(span_y);
            }
        }
    }
    let mut congestion = 0_u64;
    for left in 0..board.footprints.len() {
        for right in (left + 1)..board.footprints.len() {
            if board.footprints[left]
                .courtyard_bounds()
                .zip(board.footprints[right].courtyard_bounds())
                .is_some_and(|(a, b)| a.intersects(b))
            {
                congestion += 1;
            }
        }
    }
    let thermal = board
        .footprints
        .iter()
        .map(|footprint| footprint.height.as_micrometres())
        .sum();
    let assembly_penalty = board
        .footprints
        .iter()
        .filter(|footprint| footprint.side == jingwei_pcb::Side::Back)
        .count()
        .try_into()
        .unwrap_or(u64::MAX);
    let cost_breakdown = [
        ("ratsnest".to_owned(), i128::from(ratsnest)),
        ("congestion".to_owned(), i128::from(congestion) * 1_000_000),
        ("critical-loop".to_owned(), loop_proxy),
        ("thermal".to_owned(), i128::from(thermal)),
        (
            "assembly".to_owned(),
            i128::from(assembly_penalty) * 1_000_000,
        ),
    ]
    .into();
    PlacementMetrics {
        ratsnest_micrometres: ratsnest,
        congestion_pairs: congestion,
        loop_area_proxy: loop_proxy,
        thermal_proxy: thermal,
        assembly_penalty,
        cost_breakdown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jingwei_geometry::{Angle, Polygon};
    use jingwei_pcb::{BoardDefinition, Footprint, PcbRules, Side, Stackup};
    use std::collections::BTreeMap;

    fn board() -> Board {
        let outline = Polygon::rectangle(
            Point::from_millimetres(0, 0),
            Point::from_millimetres(60, 40),
        );
        let footprint = |id: &str, control, position| Footprint {
            id: id.to_owned(),
            logical_component_id: format!("logical:{id}"),
            designator: id.to_owned(),
            value: "fixture".to_owned(),
            manufacturer: "fixture".to_owned(),
            mpn: format!("fixture-{id}"),
            footprint_name: "fixture".to_owned(),
            placement_role: "ordinary".to_owned(),
            position,
            rotation: Angle::ZERO,
            side: Side::Front,
            body_size: (Length::from_millimetres(2), Length::from_millimetres(2)),
            courtyard_margin: Length::from_micrometres(250),
            height: Length::from_millimetres(1),
            edit_control: control,
            locked_position: position.filter(|_| control == EditControl::Locked),
        };
        Board {
            schema_version: "pcb-candidate-v0.1".to_owned(),
            source_revision: "logical".to_owned(),
            candidate_revision: "pcb".to_owned(),
            definition: BoardDefinition {
                outline,
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
            footprints: vec![
                footprint(
                    "J1",
                    EditControl::Locked,
                    Some(Point::from_millimetres(3, 20)),
                ),
                footprint("R1", EditControl::AiEditable, None),
            ],
            pads: vec![],
            nets: BTreeMap::default(),
            net_priorities: BTreeMap::default(),
            net_classes: vec![],
            tracks: vec![],
            vias: vec![],
            zones: vec![],
            keepouts: vec![],
            texts: vec![],
            pin_pad_map: BTreeMap::default(),
            evidence_refs: vec![],
        }
    }

    #[test]
    fn placement_is_reproducible_and_preserves_locked_objects() {
        let first = place_baseline(&board(), &PlacementRequest::default());
        let second = place_baseline(&board(), &PlacementRequest::default());
        assert_eq!(first, second);
        assert_eq!(
            first.board.footprints[0].position,
            Some(Point::from_millimetres(3, 20))
        );
        assert!(first.eligible());
    }

    #[test]
    fn hard_conflict_is_never_eligible_for_soft_ranking() {
        let request = PlacementRequest {
            region: Some(BoundingBox::new(
                Point::from_millimetres(0, 0),
                Point::from_millimetres(1, 1),
            )),
            ..PlacementRequest::default()
        };
        let candidate = place_baseline(&board(), &request);
        assert!(!candidate.eligible());
        assert!(!candidate.hard_violations.is_empty());
    }
}
