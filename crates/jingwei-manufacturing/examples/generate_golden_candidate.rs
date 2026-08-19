//! Materializes the checked-in golden Manufacturing Candidate and independent report.

use jingwei_drc::run_native_drc;
use jingwei_geometry::{Length, Point, Polygon};
use jingwei_manufacturing::{ExportRequest, export_candidate, load_profile, verify_candidate};
use jingwei_pcb::{
    Board, BoardDefinition, CopperLayer, LayerKind, LogicalNetlist, MechanicalFeature, Stackup,
};
use jingwei_placement::{PlacementRequest, place_baseline};
use jingwei_routing::{ReferenceRouter, RouteRequest, RouteStatus, RouterBackend};
use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repository = crate_dir.join("../..").canonicalize()?;
    let profile = load_profile(
        &repository.join("profiles/manufacturers/jlcpcb-standard-v0.yaml"),
        1024 * 1024,
    )?;
    let logical: LogicalNetlist = serde_json::from_slice(&fs::read(
        repository.join("testdata/physical/logical-netlist-v0/golden-board.json"),
    )?)?;
    let mut board = Board::from_logical(&logical, golden_board_definition(), profile.pcb_rules())?;
    let placement = place_baseline(
        &board,
        &PlacementRequest {
            seed: 0x4a57_3034,
            ..PlacementRequest::default()
        },
    );
    if !placement.eligible() {
        return Err(io::Error::other(format!(
            "golden placement has hard violations: {:?}",
            placement.hard_violations
        ))
        .into());
    }
    board = placement.board;
    let route = ReferenceRouter
        .route(
            RouteRequest {
                board,
                included_net_ids: vec![],
                excluded_net_ids: vec![],
                seed: 0x4a57_3034,
                time_budget_seconds: 10,
                critical_only: false,
            },
            CancellationToken::new(),
        )
        .await;
    if route.status != RouteStatus::Completed {
        return Err(io::Error::other(format!(
            "golden reference route did not complete: {:?}",
            route.hard_findings
        ))
        .into());
    }
    let board = route.board;
    let drc = run_native_drc(&board);
    if drc.error_count() != 0 {
        return Err(io::Error::other(format!(
            "golden native DRC has {} errors: {}",
            drc.error_count(),
            serde_json::to_string_pretty(&drc.findings)?
        ))
        .into());
    }
    let candidate = export_candidate(
        &board,
        &profile,
        &drc,
        &ExportRequest {
            candidate_root: repository.join("testdata/physical/candidates"),
            candidate_label: "golden".to_owned(),
            variant: "default".to_owned(),
            include_step: true,
        },
    )?;
    let mut verification = verify_candidate(&candidate.directory, &board, &profile);
    if !verification.valid_manufacturing_candidate {
        return Err(io::Error::other(format!(
            "golden manufacturing candidate did not verify: {}",
            serde_json::to_string_pretty(&verification.findings)?
        ))
        .into());
    }
    verification.candidate_directory = candidate
        .directory
        .strip_prefix(&repository)?
        .display()
        .to_string()
        .replace('\\', "/");
    let reports = repository.join("testdata/physical/reports");
    fs::create_dir_all(&reports)?;
    fs::write(
        reports.join("golden-package-verification.json"),
        serde_json::to_vec_pretty(&verification)?,
    )?;
    println!("candidate={}", candidate.directory.display());
    println!("manifest_sha256={}", candidate.manifest_sha256);
    println!("native_drc_errors={}", drc.error_count());
    println!(
        "package_valid={}",
        verification.valid_manufacturing_candidate
    );
    Ok(())
}

fn golden_board_definition() -> BoardDefinition {
    BoardDefinition {
        outline: Polygon::rectangle(
            Point::from_millimetres(0, 0),
            Point::from_millimetres(60, 40),
        ),
        cutouts: vec![],
        mounting_features: vec![
            mounting_hole("H1", 3, 3),
            mounting_hole("H2", 57, 3),
            mounting_hole("H3", 3, 37),
            mounting_hole("H4", 57, 37),
        ],
        stackup: Stackup {
            board_thickness: Length::from_micrometres(1_600),
            layers: vec![
                layer("F.Cu", LayerKind::Copper, 35),
                layer("B.Cu", LayerKind::Copper, 35),
                layer("F.Mask", LayerKind::SolderMask, 0),
                layer("B.Mask", LayerKind::SolderMask, 0),
                layer("F.SilkS", LayerKind::Silkscreen, 0),
                layer("B.SilkS", LayerKind::Silkscreen, 0),
                layer("Edge.Cuts", LayerKind::EdgeCuts, 0),
            ],
            surface_finish: "HASL lead-free".to_owned(),
        },
        regions: vec![],
        origin: Point::from_millimetres(0, 0),
    }
}

fn mounting_hole(id: &str, x_mm: i64, y_mm: i64) -> MechanicalFeature {
    MechanicalFeature {
        id: format!("mechanical:{id}"),
        center: Point::from_millimetres(x_mm, y_mm),
        drill: Length::from_millimetres(3),
        slot_length: Length::ZERO,
        plated: false,
        locked: true,
    }
}

fn layer(name: &str, kind: LayerKind, copper_um: i64) -> CopperLayer {
    CopperLayer {
        id: format!("layer:{name}"),
        name: name.to_owned(),
        kind,
        copper_thickness: Length::from_micrometres(copper_um),
    }
}
