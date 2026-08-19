#![allow(clippy::format_push_string)]

use crate::{MANIFEST_SCHEMA_VERSION, ManufacturerProfile, validate_profile};
use jingwei_drc::DrcReport;
use jingwei_geometry::{Length, Point};
use jingwei_kicad_pcb::{KICAD_PROJECTION_VERSION, ProjectionResult, project_board};
use jingwei_pcb::{Board, Side};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

/// Manufacturing candidate export error.
#[derive(Debug, Error)]
pub enum ManufacturingError {
    /// I/O failed.
    #[error("manufacturing export I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization failed.
    #[error("manufacturing export serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    /// Profile is incomplete.
    #[error("manufacturer profile is not usable for the golden test: {0}")]
    InvalidProfile(String),
    /// Native DRC is stale or contains errors.
    #[error("native DRC gate is not satisfied: {0}")]
    DrcGate(String),
    /// Candidate label is unsafe.
    #[error("invalid candidate label: {0}")]
    InvalidLabel(String),
    /// Immutable output already exists.
    #[error("candidate directory already exists: {0}")]
    CandidateExists(PathBuf),
    /// A relative package path is unsafe.
    #[error("unsafe package path: {0}")]
    UnsafePath(String),
}

/// Manufacturing export request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportRequest {
    /// Parent directory containing immutable candidate directories.
    pub candidate_root: PathBuf,
    /// Human-readable candidate label restricted to safe filename characters.
    pub candidate_label: String,
    /// Assembly variant.
    pub variant: String,
    /// Whether to emit the native minimal STEP candidate.
    pub include_step: bool,
}

/// One manifest file entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    /// Logical role.
    pub role: String,
    /// Safe relative path.
    pub path: String,
    /// SHA-256 digest.
    pub sha256: String,
    /// Byte count.
    pub bytes: u64,
}

/// Immutable Manufacturing Candidate manifest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Schema version.
    pub schema_version: String,
    /// Stable candidate ID.
    pub candidate_id: String,
    /// Deliberately limited maturity label.
    pub maturity: String,
    /// Source logical revision.
    pub source_revision: String,
    /// PCB candidate revision.
    pub design_revision: String,
    /// Exact input board hash.
    pub board_sha256: String,
    /// Manufacturer profile ID.
    pub manufacturer_profile_id: String,
    /// Manufacturer profile version.
    pub manufacturer_profile_version: String,
    /// Assembly variant.
    pub variant: String,
    /// Pinned generators/rules.
    pub toolchain: BTreeMap<String, String>,
    /// Validation state for this exact revision.
    pub validation: BTreeMap<String, String>,
    /// Sorted package outputs.
    pub outputs: Vec<ManifestEntry>,
    /// Gates that remain open after package verification.
    pub open_gates: Vec<String>,
}

/// Exported candidate metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManufacturingCandidate {
    /// Immutable candidate directory.
    pub directory: PathBuf,
    /// Manifest path.
    pub manifest_path: PathBuf,
    /// Hash of the manifest file.
    pub manifest_sha256: String,
    /// Parsed manifest.
    pub manifest: Manifest,
}

/// Exports an immutable, same-revision manufacturing candidate.
///
/// # Errors
///
/// Returns [`ManufacturingError`] when the profile or native DRC gate is
/// invalid, an output path is unsafe, the immutable destination already exists,
/// or an I/O/serialization operation fails.
pub fn export_candidate(
    board: &Board,
    profile: &ManufacturerProfile,
    native_drc: &DrcReport,
    request: &ExportRequest,
) -> Result<ManufacturingCandidate, ManufacturingError> {
    let profile_validation = validate_profile(profile);
    if !profile_validation.usable_for_golden_test {
        let summary = profile_validation
            .findings
            .iter()
            .filter(|finding| finding.blocking)
            .map(|finding| finding.message.clone())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ManufacturingError::InvalidProfile(summary));
    }
    if native_drc.candidate_revision != board.candidate_revision {
        return Err(ManufacturingError::DrcGate(
            "report revision differs from board revision".to_owned(),
        ));
    }
    if native_drc.board_hash != board.deterministic_hash() {
        return Err(ManufacturingError::DrcGate(
            "report board hash differs from current candidate".to_owned(),
        ));
    }
    if native_drc.error_count() != 0 {
        return Err(ManufacturingError::DrcGate(format!(
            "{} native error findings remain",
            native_drc.error_count()
        )));
    }
    validate_component(&request.candidate_label)?;
    validate_component(&request.variant)?;
    fs::create_dir_all(&request.candidate_root)?;
    let candidate_id = format!(
        "{}--{}",
        safe_component(&board.candidate_revision),
        request.candidate_label
    );
    let final_directory = request.candidate_root.join(&candidate_id);
    if final_directory.exists() {
        return Err(ManufacturingError::CandidateExists(final_directory));
    }
    let staging = request
        .candidate_root
        .join(format!(".staging-{candidate_id}"));
    if staging.exists() {
        return Err(ManufacturingError::CandidateExists(staging));
    }
    fs::create_dir(&staging)?;

    let export_result = export_into(board, profile, native_drc, request, &candidate_id, &staging);
    let (manifest, manifest_hash) = match export_result {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    };
    fs::rename(&staging, &final_directory)?;
    Ok(ManufacturingCandidate {
        manifest_path: final_directory.join("manifest.json"),
        directory: final_directory,
        manifest_sha256: manifest_hash,
        manifest,
    })
}

#[allow(clippy::too_many_lines)]
fn export_into(
    board: &Board,
    profile: &ManufacturerProfile,
    native_drc: &DrcReport,
    request: &ExportRequest,
    candidate_id: &str,
    root: &Path,
) -> Result<(Manifest, String), ManufacturingError> {
    let projection = project_board(board);
    let mut outputs = Vec::new();
    write_generated(
        root,
        "source/board-candidate.json",
        "jir-pcb-candidate",
        &serde_json::to_vec_pretty(board)?,
        &mut outputs,
    )?;
    write_generated(
        root,
        "source/revision.json",
        "revision-reference",
        &serde_json::to_vec_pretty(&serde_json::json!({
            "sourceRevision": board.source_revision,
            "candidateRevision": board.candidate_revision,
            "boardSha256": board.deterministic_hash(),
            "profile": format!("{}@{}", profile.id, profile.version),
        }))?,
        &mut outputs,
    )?;
    write_generated(
        root,
        "projection/board.kicad_pcb",
        "kicad-pcb-projection",
        projection.content.as_bytes(),
        &mut outputs,
    )?;
    write_generated(
        root,
        "projection/object-map.json",
        "projection-object-map",
        &serde_json::to_vec_pretty(&projection.object_map)?,
        &mut outputs,
    )?;
    write_generated(
        root,
        "projection/coverage.json",
        "projection-coverage",
        &serde_json::to_vec_pretty(&projection.coverage)?,
        &mut outputs,
    )?;

    for mapping in &profile.outputs {
        let bytes = output_content(board, profile, &projection, &mapping.role)?;
        write_generated(root, &mapping.path, &mapping.role, &bytes, &mut outputs)?;
    }
    write_generated(
        root,
        "assembly/assembly-preview.svg",
        "assembly-preview",
        assembly_svg(board).as_bytes(),
        &mut outputs,
    )?;
    if request.include_step {
        write_generated(
            root,
            "mechanical/board.step",
            "step-native-candidate",
            step_candidate(board).as_bytes(),
            &mut outputs,
        )?;
    }
    write_generated(
        root,
        "reports/native-drc.json",
        "native-drc-report",
        &serde_json::to_vec_pretty(native_drc)?,
        &mut outputs,
    )?;
    write_generated(
        root,
        "reports/reference-gates.json",
        "reference-gate-report",
        &serde_json::to_vec_pretty(&serde_json::json!({
            "kicad": {
                "status": "not-executed-by-exporter",
                "requiredVersion": "10.0.5",
                "gateSatisfied": false
            },
            "freerouting": {
                "status": "not-required-for-native-reference-candidate",
                "gateSatisfied": false
            },
            "note": "Run pinned external adapters separately and attach exact command/version/hash/exit-code evidence."
        }))?,
        &mut outputs,
    )?;
    outputs.sort_by(|left, right| left.path.cmp(&right.path));
    let validation = BTreeMap::from([
        (
            "nativeDrc".to_owned(),
            "passed-with-explicit-coverage".to_owned(),
        ),
        ("kicadReferenceDrc".to_owned(), "not-executed".to_owned()),
        (
            "packageReadback".to_owned(),
            "pending-independent-verifier".to_owned(),
        ),
    ]);
    let toolchain = BTreeMap::from([
        (
            "jingweiManufacturing".to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
        ),
        (
            "jingweiNativeDrc".to_owned(),
            native_drc.rule_set_version.clone(),
        ),
        (
            "kicadProjection".to_owned(),
            KICAD_PROJECTION_VERSION.to_owned(),
        ),
        (
            "manufacturerProfile".to_owned(),
            format!("{}@{}", profile.id, profile.version),
        ),
    ]);
    let manifest = Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_owned(),
        candidate_id: candidate_id.to_owned(),
        maturity: "L3-manufacturing-candidate-at-most".to_owned(),
        source_revision: board.source_revision.clone(),
        design_revision: board.candidate_revision.clone(),
        board_sha256: board.deterministic_hash(),
        manufacturer_profile_id: profile.id.clone(),
        manufacturer_profile_version: profile.version.clone(),
        variant: request.variant.clone(),
        toolchain,
        validation,
        outputs,
        open_gates: vec![
            "pinned KiCad 10.0.5 reference DRC/export not executed in native exporter".to_owned(),
            "independent package verifier must pass after manifest creation".to_owned(),
            "manufacturer DFM not performed".to_owned(),
            "physical prototype not built or tested".to_owned(),
            "named engineering and release signoffs missing".to_owned(),
        ],
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let manifest_path = root.join("manifest.json");
    fs::write(&manifest_path, &manifest_bytes)?;
    let manifest_hash = sha256(&manifest_bytes);
    fs::write(
        root.join("manifest.sha256"),
        format!("{manifest_hash}  manifest.json\n"),
    )?;
    Ok((manifest, manifest_hash))
}

fn output_content(
    board: &Board,
    profile: &ManufacturerProfile,
    projection: &ProjectionResult,
    role: &str,
) -> Result<Vec<u8>, ManufacturingError> {
    let content = match role {
        "gerber-copper-front" => gerber_copper(board, "F.Cu"),
        "gerber-copper-back" => gerber_copper(board, "B.Cu"),
        "gerber-mask-front"
        | "gerber-mask-back"
        | "gerber-silkscreen-front"
        | "gerber-silkscreen-back" => gerber_empty_layer(role),
        "gerber-outline" => gerber_outline(board),
        "gerber-job" => gerber_job(profile, board)?,
        "drill" => excellon_drill(board),
        "ipc356" => ipc356(board),
        "bom" => bom_csv(board),
        "cpl" => cpl_csv(board, profile),
        other => {
            return Err(ManufacturingError::InvalidProfile(format!(
                "no native exporter for profile role {other}"
            )));
        }
    };
    let _ = projection;
    Ok(content.into_bytes())
}

fn write_generated(
    root: &Path,
    relative: &str,
    role: &str,
    bytes: &[u8],
    outputs: &mut Vec<ManifestEntry>,
) -> Result<(), ManufacturingError> {
    let relative_path = checked_relative_path(relative)?;
    let destination = root.join(&relative_path);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&destination, bytes)?;
    outputs.push(ManifestEntry {
        role: role.to_owned(),
        path: relative_path.to_string_lossy().replace('\\', "/"),
        sha256: sha256(bytes),
        bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
    });
    Ok(())
}

fn checked_relative_path(value: &str) -> Result<PathBuf, ManufacturingError> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ManufacturingError::UnsafePath(value.to_owned()));
    }
    Ok(path.to_path_buf())
}

fn validate_component(value: &str) -> Result<(), ManufacturingError> {
    if value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(ManufacturingError::InvalidLabel(value.to_owned()));
    }
    Ok(())
}

fn safe_component(value: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
            output.push(character);
            separator = false;
        } else if !separator {
            output.push('-');
            separator = true;
        }
    }
    output.trim_matches('-').to_owned()
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn gerber_header(generation_software: &str) -> String {
    format!(
        "G04 Jingwei Manufacturing Candidate*\nG04 GenerationSoftware,{generation_software}*\n%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.200000*%\nD10*\n"
    )
}

fn gerber_copper(board: &Board, layer: &str) -> String {
    let mut output = gerber_header(env!("CARGO_PKG_VERSION"));
    for track in board.tracks.iter().filter(|track| track.layer_id == layer) {
        output.push_str(&format!("G04 NET,{}*\n", board.nets[&track.net_id]));
        for segment in track.path.segments() {
            output.push_str(&format!("{}D02*\n", gerber_xy(segment.start)));
            output.push_str(&format!("{}D01*\n", gerber_xy(segment.end)));
        }
    }
    for pad in board.pads.iter().filter(|pad| pad.center.is_some()) {
        let footprint_side = board
            .footprints
            .iter()
            .find(|footprint| footprint.id == pad.footprint_id)
            .map(|footprint| footprint.side);
        if (layer == "F.Cu" && footprint_side != Some(Side::Back))
            || (layer == "B.Cu" && pad.drill != Length::ZERO)
        {
            output.push_str(&format!("{}D03*\n", gerber_xy(pad.center.unwrap())));
        }
    }
    output.push_str("M02*\n");
    output
}

fn gerber_empty_layer(role: &str) -> String {
    let mut output = gerber_header(role);
    output.push_str("G04 Intentionally empty native Phase 0 layer*\nM02*\n");
    output
}

fn gerber_outline(board: &Board) -> String {
    let mut output = gerber_header("jingwei-outline-v1");
    output.push_str("G04 PROFILE*\n");
    for edge in board.definition.outline.edges() {
        output.push_str(&format!("{}D02*\n", gerber_xy(edge.start)));
        output.push_str(&format!("{}D01*\n", gerber_xy(edge.end)));
    }
    for cutout in &board.definition.cutouts {
        output.push_str("G04 CUTOUT*\n");
        for edge in cutout.edges() {
            output.push_str(&format!("{}D02*\n", gerber_xy(edge.start)));
            output.push_str(&format!("{}D01*\n", gerber_xy(edge.end)));
        }
    }
    output.push_str("M02*\n");
    output
}

fn gerber_xy(point: Point) -> String {
    format!(
        "X{}Y{}",
        point.x.as_micrometres().saturating_mul(1_000),
        point.y.as_micrometres().saturating_mul(1_000)
    )
}

fn gerber_job(profile: &ManufacturerProfile, board: &Board) -> Result<String, ManufacturingError> {
    let files: Vec<_> = profile
        .outputs
        .iter()
        .filter(|output| output.role.starts_with("gerber-") && output.role != "gerber-job")
        .map(|output| {
            serde_json::json!({
                "Path": output.path,
                "FileFunction": output.role,
            })
        })
        .collect();
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "Header": {
            "GenerationSoftware": { "Vendor": "Jingwei-Agent", "Application": "jingwei-manufacturing", "Version": env!("CARGO_PKG_VERSION") },
            "CreationDate": "deterministic-candidate-no-wall-clock"
        },
        "GeneralSpecs": {
            "ProjectId": board.candidate_revision,
            "LayerNumber": board.definition.stackup.layers.iter().filter(|layer| matches!(&layer.kind, jingwei_pcb::LayerKind::Copper)).count().max(2)
        },
        "FilesAttributes": files
    }))?)
}

fn excellon_drill(board: &Board) -> String {
    let mut hits = Vec::new();
    for feature in &board.definition.mounting_features {
        hits.push((feature.drill, feature.center, feature.plated));
    }
    for pad in &board.pads {
        if pad.drill != Length::ZERO
            && let Some(center) = pad.center
        {
            hits.push((pad.drill, center, pad.plated));
        }
    }
    for via in &board.vias {
        hits.push((via.drill, via.position, true));
    }
    let diameters: BTreeSet<_> = hits.iter().map(|(diameter, _, _)| *diameter).collect();
    let tools: BTreeMap<_, _> = diameters
        .into_iter()
        .enumerate()
        .map(|(index, diameter)| (diameter, index + 1))
        .collect();
    let mut output = String::from("M48\n; Jingwei Manufacturing Candidate\nMETRIC,TZ\n");
    for (diameter, tool) in &tools {
        output.push_str(&format!("T{tool:02}C{}\n", format_mm(*diameter)));
    }
    output.push_str("%\nG90\nM71\n");
    let mut current_tool = 0;
    hits.sort_by_key(|(diameter, point, plated)| (*diameter, point.x, point.y, *plated));
    for (diameter, point, plated) in hits {
        let tool = tools[&diameter];
        if current_tool != tool {
            output.push_str(&format!("T{tool:02}\n"));
            current_tool = tool;
        }
        output.push_str(&format!(
            ";TYPE={}\nX{}Y{}\n",
            if plated { "PTH" } else { "NPTH" },
            point.x.as_micrometres(),
            point.y.as_micrometres()
        ));
    }
    output.push_str("M30\n");
    output
}

fn ipc356(board: &Board) -> String {
    let mut output = format!(
        "C  IPC-D-356A Jingwei Manufacturing Candidate\nC  SOURCE_REVISION {}\nC  DESIGN_REVISION {}\nP  UNITS CUST 0\n",
        board.source_revision, board.candidate_revision
    );
    for pad in board.pads.iter().filter(|pad| pad.center.is_some()) {
        let net = pad
            .net_id
            .as_ref()
            .and_then(|id| board.nets.get(id))
            .map_or("N/C", String::as_str);
        let center = pad.center.unwrap();
        output.push_str(&format!(
            "317{net:<14} {id:<20} X{x:+07}Y{y:+07}\n",
            id = pad.id,
            x = center.x.as_micrometres(),
            y = center.y.as_micrometres()
        ));
    }
    output.push_str("999\n");
    output
}

fn bom_csv(board: &Board) -> String {
    let mut output = String::from("Designator,Comment,Footprint,Manufacturer,MPN,Variant\n");
    let mut footprints = board.footprints.clone();
    footprints.sort_by(|left, right| left.designator.cmp(&right.designator));
    for footprint in footprints {
        output.push_str(&format!(
            "{},{},{},{},{},default\n",
            csv(&footprint.designator),
            csv(&footprint.value),
            csv(&footprint.footprint_name),
            csv(&footprint.manufacturer),
            csv(&footprint.mpn)
        ));
    }
    output
}

fn cpl_csv(board: &Board, profile: &ManufacturerProfile) -> String {
    let mut output = "Designator,Mid X,Mid Y,Layer,Rotation,Unit,Origin\n".to_owned();
    let mut footprints: Vec<_> = board
        .footprints
        .iter()
        .filter(|footprint| footprint.position.is_some())
        .collect();
    footprints.sort_by(|left, right| left.designator.cmp(&right.designator));
    for footprint in footprints {
        let position = footprint.position.unwrap();
        output.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            csv(&footprint.designator),
            format_mm(position.x),
            format_mm(position.y),
            if footprint.side == Side::Front {
                "Top"
            } else {
                "Bottom"
            },
            footprint.rotation.as_microdegrees() / 1_000_000,
            profile.coordinates.unit,
            profile.coordinates.origin
        ));
    }
    output
}

fn assembly_svg(board: &Board) -> String {
    let bounds = board.definition.outline.bounds();
    let width = bounds.max.x.as_micrometres() - bounds.min.x.as_micrometres();
    let height = bounds.max.y.as_micrometres() - bounds.min.y.as_micrometres();
    let mut output = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" role=\"img\" aria-label=\"Assembly candidate {}\">\n<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"black\"/>\n",
        bounds.min.x.as_micrometres(),
        bounds.min.y.as_micrometres(),
        width,
        height,
        xml(&board.candidate_revision),
        bounds.min.x.as_micrometres(),
        bounds.min.y.as_micrometres(),
        width,
        height
    );
    for footprint in &board.footprints {
        if let Some(body) = footprint.body_bounds() {
            output.push_str(&format!(
                "<rect id=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"blue\"/><text x=\"{}\" y=\"{}\">{}</text>\n",
                xml(&footprint.id), body.min.x.as_micrometres(), body.min.y.as_micrometres(),
                body.max.x.as_micrometres() - body.min.x.as_micrometres(),
                body.max.y.as_micrometres() - body.min.y.as_micrometres(),
                body.min.x.as_micrometres(), body.min.y.as_micrometres(), xml(&footprint.designator)
            ));
        }
    }
    output.push_str("</svg>\n");
    output
}

fn step_candidate(board: &Board) -> String {
    format!(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('Jingwei Manufacturing Candidate'),'2;1');\nFILE_NAME('{}','deterministic-candidate',('Jingwei-Agent'),(''),'', 'jingwei-manufacturing','');\nFILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));\nENDSEC;\nDATA;\n#1=PRODUCT('{}','{}','Manufacturing Candidate - not Production Release',());\nENDSEC;\nEND-ISO-10303-21;\n",
        step_string(&board.candidate_revision),
        step_string(&board.candidate_revision),
        step_string(&board.source_revision)
    )
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

fn csv(value: &str) -> String {
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\n' | '\r'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn step_string(value: &str) -> String {
    value.replace('\'', "''")
}
