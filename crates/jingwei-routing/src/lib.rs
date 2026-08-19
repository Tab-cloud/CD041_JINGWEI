//! Routing candidates and replaceable router adapters.

mod freerouting;

pub use freerouting::{
    FreeroutingAvailability, FreeroutingConfig, export_specctra_dsn, probe_freerouting,
};

use async_trait::async_trait;
use jingwei_geometry::{Length, Point, Polyline};
use jingwei_pcb::{Board, EditControl, Track, Via};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use tokio_util::sync::CancellationToken;

/// Deterministic reference router version.
pub const REFERENCE_ROUTER_VERSION: &str = "manhattan-reference-router-v1";

/// Router capability report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouterCapabilities {
    /// Adapter ID.
    pub backend_id: String,
    /// Pinned adapter/backend version.
    pub version: String,
    /// Whether the route operation observes cancellation.
    pub cancellation: bool,
    /// Supported layer count.
    pub maximum_layers: u8,
    /// Supported model features.
    pub features: Vec<String>,
    /// Known uncovered capabilities.
    pub coverage_gaps: Vec<String>,
}

/// Route request over an isolated board value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteRequest {
    /// Input board candidate.
    pub board: Board,
    /// Included nets; empty means all nets.
    pub included_net_ids: Vec<String>,
    /// Excluded nets.
    pub excluded_net_ids: Vec<String>,
    /// Seed recorded for reproducibility.
    pub seed: u64,
    /// Maximum route duration expected by an external adapter.
    pub time_budget_seconds: u64,
    /// Whether only critical nets (priority <= 10) are included.
    pub critical_only: bool,
}

/// Terminal state of a route attempt.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteStatus {
    /// Candidate was generated for all included nets.
    Completed,
    /// At least one included net remains unrouted.
    Unrouted,
    /// Candidate was generated but intentionally violates profile geometry.
    Violating,
    /// Cancellation was observed.
    Canceled,
    /// Backend process or fixture crashed.
    Crashed,
    /// Backend outcome could not be reconciled.
    Unknown,
}

/// Route metrics.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteMetrics {
    /// Added track count.
    pub track_count: usize,
    /// Added via count.
    pub via_count: usize,
    /// Manhattan centerline length.
    pub routed_micrometres: i64,
    /// Nets left unrouted.
    pub unrouted_net_ids: Vec<String>,
}

/// Candidate-only route result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteCandidate {
    /// Router terminal state.
    pub status: RouteStatus,
    /// Backend identity.
    pub backend_id: String,
    /// Backend/algorithm version.
    pub backend_version: String,
    /// Input board hash.
    pub input_hash: String,
    /// Seed.
    pub seed: u64,
    /// Isolated result board.
    pub board: Board,
    /// Metrics.
    pub metrics: RouteMetrics,
    /// Hard findings that must be resolved before candidate selection.
    pub hard_findings: Vec<String>,
    /// Explicit coverage gaps.
    pub coverage_gaps: Vec<String>,
}

/// Replaceable asynchronous router backend.
#[async_trait]
pub trait RouterBackend: Send + Sync {
    /// Reports backend capabilities without executing a route.
    fn capabilities(&self) -> RouterCapabilities;

    /// Produces an isolated candidate and observes cancellation.
    async fn route(&self, request: RouteRequest, cancel: CancellationToken) -> RouteCandidate;
}

/// Built-in deterministic Manhattan reference router.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReferenceRouter;

#[async_trait]
impl RouterBackend for ReferenceRouter {
    fn capabilities(&self) -> RouterCapabilities {
        RouterCapabilities {
            backend_id: "jingwei-reference-router".to_owned(),
            version: REFERENCE_ROUTER_VERSION.to_owned(),
            cancellation: true,
            maximum_layers: 2,
            features: vec![
                "ordinary-net".to_owned(),
                "power-width".to_owned(),
                "net-priority".to_owned(),
                "keepout-rejection".to_owned(),
            ],
            coverage_gaps: default_route_gaps(),
        }
    }

    async fn route(&self, request: RouteRequest, cancel: CancellationToken) -> RouteCandidate {
        route_reference(request, &cancel, false)
    }
}

/// Deterministic fault-injection mode.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FakeRouterMode {
    /// Successful reference route.
    Success,
    /// Leaves the final net unrouted.
    Unrouted,
    /// Uses a width below the active rule.
    Violating,
    /// Returns cancellation.
    Canceled,
    /// Simulates a contained backend crash.
    Crashed,
    /// Simulates an unknown external outcome.
    Unknown,
}

/// Required deterministic router fixture.
#[derive(Clone, Copy, Debug)]
pub struct FakeRouter {
    /// Requested terminal behavior.
    pub mode: FakeRouterMode,
}

#[async_trait]
impl RouterBackend for FakeRouter {
    fn capabilities(&self) -> RouterCapabilities {
        RouterCapabilities {
            backend_id: "jingwei-fake-router".to_owned(),
            version: "fault-fixture-v1".to_owned(),
            cancellation: true,
            maximum_layers: 2,
            features: vec!["fault-injection".to_owned()],
            coverage_gaps: default_route_gaps(),
        }
    }

    async fn route(&self, mut request: RouteRequest, cancel: CancellationToken) -> RouteCandidate {
        match self.mode {
            FakeRouterMode::Success => route_reference(request, &cancel, false),
            FakeRouterMode::Unrouted => {
                let mut candidates = selected_net_ids(&request);
                if let Some(last) = candidates.pop() {
                    request.excluded_net_ids.push(last);
                }
                let mut result = route_reference(request, &cancel, false);
                result.status = RouteStatus::Unrouted;
                result
                    .hard_findings
                    .push("fake router left a selected net unrouted".to_owned());
                result
            }
            FakeRouterMode::Violating => route_reference(request, &cancel, true),
            FakeRouterMode::Canceled => terminal_candidate(
                request,
                RouteStatus::Canceled,
                "fake router observed cancellation",
            ),
            FakeRouterMode::Crashed => {
                terminal_candidate(request, RouteStatus::Crashed, "contained fake router crash")
            }
            FakeRouterMode::Unknown => terminal_candidate(
                request,
                RouteStatus::Unknown,
                "router terminal state requires reconciliation",
            ),
        }
    }
}

#[allow(clippy::too_many_lines)]
fn route_reference(
    request: RouteRequest,
    cancel: &CancellationToken,
    violating: bool,
) -> RouteCandidate {
    let input_hash = request.board.deterministic_hash();
    if cancel.is_cancelled() {
        return terminal_candidate(
            request,
            RouteStatus::Canceled,
            "route canceled before start",
        );
    }
    let selected = selected_net_ids(&request);
    let mut board = request.board;
    let mut metrics = RouteMetrics::default();
    let mut hard_findings = Vec::new();
    for (net_index, net_id) in selected.iter().enumerate() {
        if cancel.is_cancelled() {
            hard_findings
                .push("route canceled; partial tracks are an isolated candidate".to_owned());
            return RouteCandidate {
                status: RouteStatus::Canceled,
                backend_id: "jingwei-reference-router".to_owned(),
                backend_version: REFERENCE_ROUTER_VERSION.to_owned(),
                input_hash,
                seed: request.seed,
                board,
                metrics,
                hard_findings,
                coverage_gaps: default_route_gaps(),
            };
        }
        let centers = board.pad_centers_for_net(net_id);
        if centers.len() < 2 {
            continue;
        }
        let class = board.class_for_net(net_id).cloned();
        let width = if violating {
            Length::from_micrometres(
                board
                    .rules
                    .minimum_track_width
                    .as_micrometres()
                    .saturating_sub(1),
            )
        } else {
            class
                .as_ref()
                .map_or(board.rules.minimum_track_width, |class| class.track_width)
        };
        for (segment_index, pair) in centers.windows(2).enumerate() {
            let start = pair[0].1;
            let end = pair[1].1;
            let offset = i64::try_from(net_index).unwrap_or(i64::MAX) * 500;
            let corridor_y =
                i64::midpoint(start.y.as_micrometres(), end.y.as_micrometres()) + offset;
            let points = if start.x == end.x || start.y == end.y {
                vec![start, end]
            } else {
                vec![
                    start,
                    Point::from_micrometres(start.x.as_micrometres(), corridor_y),
                    Point::from_micrometres(end.x.as_micrometres(), corridor_y),
                    end,
                ]
            };
            let path = Polyline::new(points).expect("reference route always has two endpoints");
            metrics.routed_micrometres = metrics
                .routed_micrometres
                .saturating_add(path.manhattan_length_micrometres());
            board.tracks.push(Track {
                id: format!("track:{net_id}:{segment_index}"),
                net_id: net_id.clone(),
                layer_id: "F.Cu".to_owned(),
                width,
                path,
                provenance: REFERENCE_ROUTER_VERSION.to_owned(),
                edit_control: EditControl::AiEditable,
            });
            metrics.track_count += 1;
        }
    }
    let routed: BTreeSet<_> = board
        .tracks
        .iter()
        .map(|track| track.net_id.clone())
        .collect();
    metrics.unrouted_net_ids = selected
        .iter()
        .filter(|net_id| {
            board.pad_centers_for_net(net_id).len() >= 2 && !routed.contains(net_id.as_str())
        })
        .cloned()
        .collect();
    let status = if violating {
        hard_findings.push("fixture produced track width below profile minimum".to_owned());
        RouteStatus::Violating
    } else if metrics.unrouted_net_ids.is_empty() {
        RouteStatus::Completed
    } else {
        hard_findings.push(format!(
            "unrouted nets: {}",
            metrics.unrouted_net_ids.join(",")
        ));
        RouteStatus::Unrouted
    };
    board.candidate_revision = format!(
        "{}:route:{}:{:016x}",
        board.candidate_revision, REFERENCE_ROUTER_VERSION, request.seed
    );
    RouteCandidate {
        status,
        backend_id: "jingwei-reference-router".to_owned(),
        backend_version: REFERENCE_ROUTER_VERSION.to_owned(),
        input_hash,
        seed: request.seed,
        board,
        metrics,
        hard_findings,
        coverage_gaps: default_route_gaps(),
    }
}

fn selected_net_ids(request: &RouteRequest) -> Vec<String> {
    let included: BTreeSet<_> = request.included_net_ids.iter().cloned().collect();
    let excluded: BTreeSet<_> = request.excluded_net_ids.iter().cloned().collect();
    let mut nets: Vec<_> = request
        .board
        .nets
        .keys()
        .filter(|id| included.is_empty() || included.contains(*id))
        .filter(|id| !excluded.contains(*id))
        .filter(|id| {
            !request.critical_only
                || request
                    .board
                    .net_priorities
                    .get(*id)
                    .copied()
                    .unwrap_or(u16::MAX)
                    <= 10
        })
        .cloned()
        .collect();
    nets.sort_by_key(|id| {
        (
            request
                .board
                .net_priorities
                .get(id)
                .copied()
                .unwrap_or(u16::MAX),
            id.clone(),
        )
    });
    nets
}

fn terminal_candidate(request: RouteRequest, status: RouteStatus, finding: &str) -> RouteCandidate {
    RouteCandidate {
        status,
        backend_id: "jingwei-fake-router".to_owned(),
        backend_version: "fault-fixture-v1".to_owned(),
        input_hash: request.board.deterministic_hash(),
        seed: request.seed,
        board: request.board,
        metrics: RouteMetrics::default(),
        hard_findings: vec![finding.to_owned()],
        coverage_gaps: default_route_gaps(),
    }
}

fn default_route_gaps() -> Vec<String> {
    vec![
        "differential-pair-geometry-not-solved".to_owned(),
        "length-matching-not-solved".to_owned(),
        "controlled-impedance-not-solved".to_owned(),
        "si-pi-return-path-not-proven".to_owned(),
    ]
}

/// Convenience request for critical routing while keeping the adapter replaceable.
pub async fn route_critical(
    backend: &dyn RouterBackend,
    mut request: RouteRequest,
    cancel: CancellationToken,
) -> RouteCandidate {
    request.critical_only = true;
    backend.route(request, cancel).await
}

/// Builds a via using the active profile minimums.
pub fn profile_via(board: &Board, id: &str, net_id: &str, position: Point) -> Via {
    Via {
        id: id.to_owned(),
        net_id: net_id.to_owned(),
        position,
        diameter: Length::from_micrometres(
            board.rules.minimum_drill.as_micrometres()
                + 2 * board.rules.minimum_annular_ring.as_micrometres(),
        ),
        drill: board.rules.minimum_drill,
        edit_control: EditControl::AiEditable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jingwei_geometry::Polygon;
    use jingwei_pcb::{BoardDefinition, NetClass, Pad, PcbRules, Stackup};
    use std::collections::BTreeMap;

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
            pads: vec![
                Pad {
                    id: "pad:1".to_owned(),
                    footprint_id: "fp:1".to_owned(),
                    number: "1".to_owned(),
                    offset: Point::default(),
                    center: Some(Point::from_millimetres(10, 10)),
                    size: (Length::from_millimetres(1), Length::from_millimetres(1)),
                    drill: Length::ZERO,
                    plated: false,
                    net_id: Some("net:1".to_owned()),
                    mask_expansion: Length::from_micrometres(50),
                },
                Pad {
                    id: "pad:2".to_owned(),
                    footprint_id: "fp:2".to_owned(),
                    number: "1".to_owned(),
                    offset: Point::default(),
                    center: Some(Point::from_millimetres(40, 30)),
                    size: (Length::from_millimetres(1), Length::from_millimetres(1)),
                    drill: Length::ZERO,
                    plated: false,
                    net_id: Some("net:1".to_owned()),
                    mask_expansion: Length::from_micrometres(50),
                },
            ],
            nets: BTreeMap::from([("net:1".to_owned(), "SIGNAL".to_owned())]),
            net_priorities: BTreeMap::from([("net:1".to_owned(), 20)]),
            net_classes: vec![NetClass {
                id: "default".to_owned(),
                track_width: Length::from_micrometres(200),
                clearance: Length::from_micrometres(200),
                via_diameter: Length::from_micrometres(600),
                via_drill: Length::from_micrometres(300),
                differential_gap: None,
                target_length: None,
            }],
            tracks: vec![],
            vias: vec![],
            zones: vec![],
            keepouts: vec![],
            texts: vec![],
            pin_pad_map: BTreeMap::new(),
            evidence_refs: vec![],
        }
    }

    fn request() -> RouteRequest {
        RouteRequest {
            board: board(),
            included_net_ids: vec![],
            excluded_net_ids: vec![],
            seed: 7,
            time_budget_seconds: 1,
            critical_only: false,
        }
    }

    #[tokio::test]
    async fn reference_route_is_reproducible() {
        let first = ReferenceRouter
            .route(request(), CancellationToken::new())
            .await;
        let second = ReferenceRouter
            .route(request(), CancellationToken::new())
            .await;
        assert_eq!(first, second);
        assert_eq!(first.status, RouteStatus::Completed);
    }

    #[tokio::test]
    async fn all_fault_terminal_states_are_explicit() {
        for (mode, expected) in [
            (FakeRouterMode::Canceled, RouteStatus::Canceled),
            (FakeRouterMode::Crashed, RouteStatus::Crashed),
            (FakeRouterMode::Unknown, RouteStatus::Unknown),
            (FakeRouterMode::Violating, RouteStatus::Violating),
        ] {
            let result = FakeRouter { mode }
                .route(request(), CancellationToken::new())
                .await;
            assert_eq!(result.status, expected);
            assert!(!result.hard_findings.is_empty());
        }
    }

    #[tokio::test]
    async fn pre_canceled_token_never_reports_success() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = ReferenceRouter.route(request(), cancel).await;
        assert_eq!(result.status, RouteStatus::Canceled);
    }
}
