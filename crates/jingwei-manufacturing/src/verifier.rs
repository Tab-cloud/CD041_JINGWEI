use crate::{MANIFEST_SCHEMA_VERSION, Manifest, ManufacturerProfile};
use jingwei_kicad_pcb::parse_sexpr;
use jingwei_pcb::Board;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};

/// Independent package-verification finding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageFinding {
    /// Machine-readable finding code.
    pub code: String,
    /// Whether the package is invalid until repaired/regenerated.
    pub blocking: bool,
    /// Explanation.
    pub message: String,
    /// Affected paths or design objects.
    pub object_refs: Vec<String>,
}

/// Independent read-back verification report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageVerification {
    /// Candidate directory.
    pub candidate_directory: String,
    /// Manifest revision when parseable.
    pub design_revision: Option<String>,
    /// Whether every blocking parser/consistency check passed.
    pub valid_manufacturing_candidate: bool,
    /// Parsed roles.
    pub parsed_roles: Vec<String>,
    /// Sorted findings.
    pub findings: Vec<PackageFinding>,
    /// Explicit open production gates.
    pub open_gates: Vec<String>,
}

/// Re-reads all candidate outputs with independent bounded parsers and checks consistency.
#[allow(clippy::too_many_lines)]
pub fn verify_candidate(
    directory: &Path,
    board: &Board,
    profile: &ManufacturerProfile,
) -> PackageVerification {
    let mut findings = Vec::new();
    let mut parsed_roles = Vec::new();
    let maximum = usize::try_from(board.rules.maximum_artifact_bytes).unwrap_or(usize::MAX);
    let manifest_bytes = match read_bounded(&directory.join("manifest.json"), maximum) {
        Ok(bytes) => bytes,
        Err(message) => {
            push(
                &mut findings,
                "package.manifest-unreadable",
                true,
                message,
                vec!["manifest.json".to_owned()],
            );
            return finish(directory, None, parsed_roles, findings, vec![]);
        }
    };
    let manifest: Manifest = match serde_json::from_slice(&manifest_bytes) {
        Ok(value) => value,
        Err(error) => {
            push(
                &mut findings,
                "package.manifest-invalid-json",
                true,
                error.to_string(),
                vec!["manifest.json".to_owned()],
            );
            return finish(directory, None, parsed_roles, findings, vec![]);
        }
    };
    parsed_roles.push("manifest".to_owned());
    check_manifest_identity(&manifest, board, profile, &mut findings);
    check_manifest_hash(directory, &manifest_bytes, &mut findings);

    let root = match directory.canonicalize() {
        Ok(root) => root,
        Err(error) => {
            push(
                &mut findings,
                "package.directory-unreadable",
                true,
                error.to_string(),
                vec![directory.display().to_string()],
            );
            return finish(
                directory,
                Some(manifest.design_revision),
                parsed_roles,
                findings,
                manifest.open_gates,
            );
        }
    };
    let mut roles = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut contents = BTreeMap::new();
    for entry in &manifest.outputs {
        if !roles.insert(entry.role.clone()) {
            push(
                &mut findings,
                "package.duplicate-role",
                true,
                format!("duplicate output role {}", entry.role),
                vec![entry.role.clone()],
            );
        }
        if !paths.insert(entry.path.clone()) {
            push(
                &mut findings,
                "package.duplicate-path",
                true,
                format!("duplicate output path {}", entry.path),
                vec![entry.path.clone()],
            );
        }
        let relative = Path::new(&entry.path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            push(
                &mut findings,
                "package.path-traversal",
                true,
                format!("unsafe manifest path {}", entry.path),
                vec![entry.path.clone()],
            );
            continue;
        }
        let path = directory.join(relative);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                push(
                    &mut findings,
                    "package.file-missing",
                    true,
                    error.to_string(),
                    vec![entry.path.clone()],
                );
                continue;
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            push(
                &mut findings,
                "package.non-regular-file",
                true,
                "manifest outputs must be regular files, not links/directories".to_owned(),
                vec![entry.path.clone()],
            );
            continue;
        }
        let canonical = match path.canonicalize() {
            Ok(value) => value,
            Err(error) => {
                push(
                    &mut findings,
                    "package.file-unreadable",
                    true,
                    error.to_string(),
                    vec![entry.path.clone()],
                );
                continue;
            }
        };
        if !canonical.starts_with(&root) {
            push(
                &mut findings,
                "package.path-escape",
                true,
                "resolved output escapes the candidate directory".to_owned(),
                vec![entry.path.clone()],
            );
            continue;
        }
        if metadata.len() == 0 || metadata.len() != entry.bytes {
            push(
                &mut findings,
                "package.file-size",
                true,
                format!(
                    "expected {} bytes, observed {} bytes",
                    entry.bytes,
                    metadata.len()
                ),
                vec![entry.path.clone()],
            );
        }
        let bytes = match read_bounded(&path, maximum) {
            Ok(bytes) => bytes,
            Err(message) => {
                push(
                    &mut findings,
                    "package.file-oversized",
                    true,
                    message,
                    vec![entry.path.clone()],
                );
                continue;
            }
        };
        if sha256(&bytes) != entry.sha256 {
            push(
                &mut findings,
                "package.hash-mismatch",
                true,
                "file SHA-256 does not match manifest".to_owned(),
                vec![entry.path.clone()],
            );
        }
        contents.insert(entry.role.clone(), bytes);
    }
    check_profile_mapping(&manifest, profile, &mut findings);
    parse_outputs(
        &contents,
        board,
        profile,
        maximum,
        &mut parsed_roles,
        &mut findings,
    );
    finish(
        directory,
        Some(manifest.design_revision),
        parsed_roles,
        findings,
        manifest.open_gates,
    )
}

fn check_manifest_identity(
    manifest: &Manifest,
    board: &Board,
    profile: &ManufacturerProfile,
    findings: &mut Vec<PackageFinding>,
) {
    for (code, matches, message) in [
        (
            "package.manifest-schema",
            manifest.schema_version == MANIFEST_SCHEMA_VERSION,
            "manifest schema version mismatch",
        ),
        (
            "package.source-revision",
            manifest.source_revision == board.source_revision,
            "source revision mismatch",
        ),
        (
            "package.design-revision",
            manifest.design_revision == board.candidate_revision,
            "design revision mismatch",
        ),
        (
            "package.board-hash",
            manifest.board_sha256 == board.deterministic_hash(),
            "board hash mismatch",
        ),
        (
            "package.profile-id",
            manifest.manufacturer_profile_id == profile.id,
            "manufacturer profile ID mismatch",
        ),
        (
            "package.profile-version",
            manifest.manufacturer_profile_version == profile.version,
            "manufacturer profile version mismatch",
        ),
    ] {
        if !matches {
            push(
                findings,
                code,
                true,
                message.to_owned(),
                vec!["manifest.json".to_owned()],
            );
        }
    }
    if !manifest.maturity.contains("manufacturing-candidate") {
        push(
            findings,
            "package.maturity-label",
            true,
            "manifest must identify a Manufacturing Candidate, never a Production Release"
                .to_owned(),
            vec!["manifest.json".to_owned()],
        );
    }
}

fn check_manifest_hash(
    directory: &Path,
    manifest_bytes: &[u8],
    findings: &mut Vec<PackageFinding>,
) {
    let sidecar = match fs::read_to_string(directory.join("manifest.sha256")) {
        Ok(value) => value,
        Err(error) => {
            push(
                findings,
                "package.manifest-hash-missing",
                true,
                error.to_string(),
                vec!["manifest.sha256".to_owned()],
            );
            return;
        }
    };
    let expected = sidecar.split_whitespace().next().unwrap_or_default();
    if expected != sha256(manifest_bytes) {
        push(
            findings,
            "package.manifest-hash-mismatch",
            true,
            "manifest SHA-256 sidecar does not match".to_owned(),
            vec!["manifest.json".to_owned(), "manifest.sha256".to_owned()],
        );
    }
}

fn check_profile_mapping(
    manifest: &Manifest,
    profile: &ManufacturerProfile,
    findings: &mut Vec<PackageFinding>,
) {
    for mapping in &profile.outputs {
        match manifest
            .outputs
            .iter()
            .find(|entry| entry.role == mapping.role)
        {
            Some(entry) if entry.path == mapping.path => {}
            Some(entry) => push(
                findings,
                "package.wrong-layer-file-mapping",
                true,
                format!(
                    "role {} must use {}, observed {}",
                    mapping.role, mapping.path, entry.path
                ),
                vec![entry.path.clone()],
            ),
            None => push(
                findings,
                "package.required-output-missing",
                true,
                format!("missing profile output role {}", mapping.role),
                vec![mapping.role.clone()],
            ),
        }
    }
}

#[allow(clippy::too_many_lines)]
fn parse_outputs(
    contents: &BTreeMap<String, Vec<u8>>,
    board: &Board,
    profile: &ManufacturerProfile,
    maximum: usize,
    parsed_roles: &mut Vec<String>,
    findings: &mut Vec<PackageFinding>,
) {
    for (role, bytes) in contents {
        if role.starts_with("gerber-") && role != "gerber-job" {
            parse_gerber(role, bytes, findings);
            parsed_roles.push(role.clone());
        }
    }
    if let Some(bytes) = contents.get("gerber-outline") {
        check_outline_gerber(bytes, board, findings);
    }
    if let Some(bytes) = contents.get("gerber-job") {
        if serde_json::from_slice::<serde_json::Value>(bytes).is_err() {
            push(
                findings,
                "package.gerber-job-invalid",
                true,
                "Gerber Job is not valid JSON".to_owned(),
                vec!["gerber-job".to_owned()],
            );
        } else {
            parsed_roles.push("gerber-job".to_owned());
        }
    }
    if let Some(bytes) = contents.get("drill") {
        check_drill(bytes, board, findings);
        parsed_roles.push("drill".to_owned());
    }
    if let Some(bytes) = contents.get("ipc356") {
        check_ipc356(bytes, board, findings);
        parsed_roles.push("ipc356".to_owned());
    }
    let bom = contents
        .get("bom")
        .and_then(|bytes| parse_designator_csv(bytes, "Designator", findings, "bom"));
    let cpl = contents.get("cpl").and_then(|bytes| {
        check_cpl_convention(bytes, profile, findings);
        parse_designator_csv(bytes, "Designator", findings, "cpl")
    });
    let expected: BTreeSet<_> = board
        .footprints
        .iter()
        .map(|footprint| footprint.designator.clone())
        .collect();
    if let Some(bom) = bom {
        parsed_roles.push("bom".to_owned());
        if bom != expected {
            push(
                findings,
                "package.stale-bom",
                true,
                format!("BOM designators {bom:?} differ from PCB {expected:?}"),
                vec!["bom".to_owned()],
            );
        }
    }
    if let Some(cpl) = cpl {
        parsed_roles.push("cpl".to_owned());
        if cpl != expected {
            push(
                findings,
                "package.stale-cpl",
                true,
                format!("CPL designators {cpl:?} differ from PCB {expected:?}"),
                vec!["cpl".to_owned()],
            );
        }
    }
    if let (Some(bom), Some(cpl)) = (
        contents
            .get("bom")
            .and_then(|bytes| parse_csv_set(bytes).ok()),
        contents
            .get("cpl")
            .and_then(|bytes| parse_csv_set(bytes).ok()),
    ) && bom != cpl
    {
        push(
            findings,
            "package.bom-cpl-not-closed",
            true,
            "BOM and CPL designator sets differ".to_owned(),
            vec!["bom".to_owned(), "cpl".to_owned()],
        );
    }
    if let Some(bytes) = contents.get("kicad-pcb-projection") {
        let content = String::from_utf8_lossy(bytes);
        if let Err(error) = parse_sexpr(&content, maximum) {
            push(
                findings,
                "package.kicad-projection-invalid",
                true,
                error.to_string(),
                vec!["kicad-pcb-projection".to_owned()],
            );
        } else {
            parsed_roles.push("kicad-pcb-projection".to_owned());
        }
    }
    if let Some(bytes) = contents.get("step-native-candidate") {
        let text = String::from_utf8_lossy(bytes);
        if !text.starts_with("ISO-10303-21;") || !text.ends_with("END-ISO-10303-21;\n") {
            push(
                findings,
                "package.step-invalid",
                true,
                "STEP candidate lacks ISO-10303-21 envelope".to_owned(),
                vec!["step-native-candidate".to_owned()],
            );
        } else {
            parsed_roles.push("step-native-candidate".to_owned());
        }
    }
    parsed_roles.sort();
    parsed_roles.dedup();
}

fn parse_gerber(role: &str, bytes: &[u8], findings: &mut Vec<PackageFinding>) {
    let text = String::from_utf8_lossy(bytes);
    if !text.contains("%FSLAX46Y46*%") || !text.contains("%MOMM*%") || !text.ends_with("M02*\n") {
        push(
            findings,
            "package.gerber-invalid",
            true,
            "Gerber lacks format/unit/end markers".to_owned(),
            vec![role.to_owned()],
        );
    }
}

fn check_outline_gerber(bytes: &[u8], board: &Board, findings: &mut Vec<PackageFinding>) {
    let text = String::from_utf8_lossy(bytes);
    let mut current = None;
    let mut segments = Vec::new();
    for line in text.lines() {
        if line.starts_with('X') && line.ends_with("D02*") {
            current = parse_xy(line);
        } else if line.starts_with('X')
            && line.ends_with("D01*")
            && let (Some(start), Some(end)) = (current, parse_xy(line))
        {
            segments.push((start, end));
            current = Some(end);
        }
    }
    let expected = board.definition.outline.edges().count();
    let closed = !segments.is_empty()
        && segments.windows(2).all(|pair| pair[0].1 == pair[1].0)
        && segments.last().map(|segment| segment.1) == segments.first().map(|segment| segment.0);
    if segments.len() < expected || !closed {
        push(
            findings,
            "package.outline-not-unique-closed",
            true,
            format!(
                "outline has {} parsed segments; expected at least {expected}; closed={closed}",
                segments.len()
            ),
            vec!["gerber-outline".to_owned()],
        );
    }
}

fn parse_xy(line: &str) -> Option<(i64, i64)> {
    let x_start = line.strip_prefix('X')?;
    let y_index = x_start.find('Y')?;
    let (x, rest) = x_start.split_at(y_index);
    let rest = &rest[1..];
    let d_index = rest.find('D')?;
    let y = &rest[..d_index];
    Some((x.parse().ok()?, y.parse().ok()?))
}

fn check_drill(bytes: &[u8], board: &Board, findings: &mut Vec<PackageFinding>) {
    let text = String::from_utf8_lossy(bytes);
    let observed = text
        .lines()
        .filter(|line| line.starts_with('X') && line.contains('Y'))
        .count();
    let expected = board.definition.mounting_features.len()
        + board
            .pads
            .iter()
            .filter(|pad| pad.drill.as_micrometres() > 0 && pad.center.is_some())
            .count()
        + board.vias.len();
    if !text.contains("METRIC,TZ") || !text.ends_with("M30\n") || observed != expected {
        push(
            findings,
            "package.drill-mismatch",
            true,
            format!("observed {observed} drill hits, expected {expected}"),
            vec!["drill".to_owned()],
        );
    }
}

fn check_ipc356(bytes: &[u8], board: &Board, findings: &mut Vec<PackageFinding>) {
    let text = String::from_utf8_lossy(bytes);
    if !text.contains(&format!("SOURCE_REVISION {}", board.source_revision))
        || !text.contains(&format!("DESIGN_REVISION {}", board.candidate_revision))
        || !text.ends_with("999\n")
        || board
            .pads
            .iter()
            .filter(|pad| pad.center.is_some())
            .any(|pad| !text.contains(&pad.id))
    {
        push(
            findings,
            "package.ipc356-connectivity",
            true,
            "IPC-356 revision/pad connectivity does not match the board".to_owned(),
            vec!["ipc356".to_owned()],
        );
    }
}

fn parse_designator_csv(
    bytes: &[u8],
    column: &str,
    findings: &mut Vec<PackageFinding>,
    role: &str,
) -> Option<BTreeSet<String>> {
    match parse_csv(bytes) {
        Ok((header, rows)) => {
            let Some(index) = header.iter().position(|value| value == column) else {
                push(
                    findings,
                    "package.csv-column-missing",
                    true,
                    format!("{role} lacks {column} column"),
                    vec![role.to_owned()],
                );
                return None;
            };
            let mut values = BTreeSet::new();
            for row in rows {
                if row.len() != header.len() {
                    push(
                        findings,
                        "package.csv-row-width",
                        true,
                        format!("{role} row has wrong number of columns"),
                        vec![role.to_owned()],
                    );
                    return None;
                }
                if !row[index].is_empty() && !values.insert(row[index].clone()) {
                    push(
                        findings,
                        "package.csv-duplicate-designator",
                        true,
                        format!("duplicate designator {} in {role}", row[index]),
                        vec![role.to_owned()],
                    );
                }
            }
            Some(values)
        }
        Err(message) => {
            push(
                findings,
                "package.csv-invalid",
                true,
                format!("{role}: {message}"),
                vec![role.to_owned()],
            );
            None
        }
    }
}

fn check_cpl_convention(
    bytes: &[u8],
    profile: &ManufacturerProfile,
    findings: &mut Vec<PackageFinding>,
) {
    let Ok((header, rows)) = parse_csv(bytes) else {
        return;
    };
    let index = |name: &str| header.iter().position(|value| value == name);
    let (Some(rotation), Some(unit), Some(origin), Some(layer)) = (
        index("Rotation"),
        index("Unit"),
        index("Origin"),
        index("Layer"),
    ) else {
        return;
    };
    for row in rows {
        if row.len() != header.len() {
            continue;
        }
        let angle_ok = row[rotation]
            .parse::<i64>()
            .is_ok_and(|angle| (0..360).contains(&angle));
        let mapping_ok = row[unit] == profile.coordinates.unit
            && row[origin] == profile.coordinates.origin
            && matches!(row[layer].as_str(), "Top" | "Bottom");
        if !angle_ok || !mapping_ok {
            push(
                findings,
                "package.cpl-coordinate-convention",
                true,
                "CPL unit/origin/layer/rotation is invalid".to_owned(),
                vec!["cpl".to_owned()],
            );
        }
    }
}

fn parse_csv_set(bytes: &[u8]) -> Result<BTreeSet<String>, String> {
    let (header, rows) = parse_csv(bytes)?;
    let index = header
        .iter()
        .position(|value| value == "Designator")
        .ok_or_else(|| "Designator column missing".to_owned())?;
    Ok(rows
        .into_iter()
        .filter_map(|row| row.get(index).cloned())
        .filter(|value| !value.is_empty())
        .collect())
}

fn parse_csv(bytes: &[u8]) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => record.push(std::mem::take(&mut field)),
            '\n' if !quoted => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            '\r' if !quoted => {}
            other => field.push(other),
        }
    }
    if quoted {
        return Err("unterminated quoted field".to_owned());
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    let header = records
        .first()
        .cloned()
        .ok_or_else(|| "empty CSV".to_owned())?;
    Ok((header, records.into_iter().skip(1).collect()))
}

fn read_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > u64::try_from(maximum).unwrap_or(u64::MAX) {
        return Err(format!(
            "{} is {} bytes, exceeding the {maximum} byte parser limit",
            path.display(),
            metadata.len()
        ));
    }
    fs::read(path).map_err(|error| error.to_string())
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn push(
    findings: &mut Vec<PackageFinding>,
    code: &str,
    blocking: bool,
    message: String,
    object_refs: Vec<String>,
) {
    findings.push(PackageFinding {
        code: code.to_owned(),
        blocking,
        message,
        object_refs,
    });
}

fn finish(
    directory: &Path,
    design_revision: Option<String>,
    parsed_roles: Vec<String>,
    mut findings: Vec<PackageFinding>,
    open_gates: Vec<String>,
) -> PackageVerification {
    findings.sort_by(|left, right| {
        (left.code.as_str(), left.object_refs.as_slice())
            .cmp(&(right.code.as_str(), right.object_refs.as_slice()))
    });
    PackageVerification {
        candidate_directory: directory.display().to_string(),
        design_revision,
        valid_manufacturing_candidate: findings.iter().all(|finding| !finding.blocking),
        parsed_roles,
        findings,
        open_gates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExportRequest, export_candidate, load_profile};
    use jingwei_drc::run_native_drc;
    use jingwei_geometry::{Length, Point, Polygon};
    use jingwei_pcb::{BoardDefinition, Stackup};
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temporary_root(name: &str) -> std::path::PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "jingwei-manufacturing-test-{}-{name}-{id}",
            std::process::id()
        ))
    }

    fn board(profile: &ManufacturerProfile) -> Board {
        Board {
            schema_version: "pcb-candidate-v0.1".to_owned(),
            source_revision: "logical".to_owned(),
            candidate_revision: "pcb-clean".to_owned(),
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
                    surface_finish: "HASL lead-free".to_owned(),
                },
                regions: vec![],
                origin: Point::from_millimetres(0, 0),
            },
            rules: profile.pcb_rules(),
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

    fn profile() -> ManufacturerProfile {
        load_profile(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../profiles/manufacturers/jlcpcb-standard-v0.yaml"),
            1024 * 1024,
        )
        .unwrap()
    }

    #[test]
    fn exported_candidate_is_independently_parseable_and_immutable() {
        let profile = profile();
        let board = board(&profile);
        let drc = run_native_drc(&board);
        let root = temporary_root("valid");
        let request = ExportRequest {
            candidate_root: root.clone(),
            candidate_label: "golden".to_owned(),
            variant: "default".to_owned(),
            include_step: true,
        };
        let candidate = export_candidate(&board, &profile, &drc, &request).unwrap();
        let verification = verify_candidate(&candidate.directory, &board, &profile);
        assert!(
            verification.valid_manufacturing_candidate,
            "{:?}",
            verification.findings
        );
        assert!(export_candidate(&board, &profile, &drc, &request).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_bom_is_blocking_even_when_files_still_parse() {
        let profile = profile();
        let board = board(&profile);
        let root = temporary_root("stale-bom");
        let candidate = export_candidate(
            &board,
            &profile,
            &run_native_drc(&board),
            &ExportRequest {
                candidate_root: root.clone(),
                candidate_label: "negative".to_owned(),
                variant: "default".to_owned(),
                include_step: false,
            },
        )
        .unwrap();
        fs::write(
            candidate.directory.join("assembly/bom.csv"),
            "Designator,Comment,Footprint,Manufacturer,MPN,Variant\nX1,stale,x,x,x,default\n",
        )
        .unwrap();
        let verification = verify_candidate(&candidate.directory, &board, &profile);
        assert!(!verification.valid_manufacturing_candidate);
        assert!(verification.findings.iter().any(|finding| {
            matches!(
                finding.code.as_str(),
                "package.hash-mismatch" | "package.stale-bom"
            )
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn traversal_in_manifest_is_rejected_before_file_access() {
        let profile = profile();
        let board = board(&profile);
        let root = temporary_root("traversal");
        let candidate = export_candidate(
            &board,
            &profile,
            &run_native_drc(&board),
            &ExportRequest {
                candidate_root: root.clone(),
                candidate_label: "negative".to_owned(),
                variant: "default".to_owned(),
                include_step: false,
            },
        )
        .unwrap();
        let mut manifest = candidate.manifest;
        manifest.outputs[0].path = "../outside.bin".to_owned();
        fs::write(
            candidate.directory.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let verification = verify_candidate(&candidate.directory, &board, &profile);
        assert!(
            verification
                .findings
                .iter()
                .any(|finding| finding.code == "package.path-traversal")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wrong_profile_path_and_cpl_convention_are_blocking() {
        let profile = profile();
        let board = board(&profile);

        let mapping_root = temporary_root("wrong-mapping");
        let mapping_candidate = export_candidate(
            &board,
            &profile,
            &run_native_drc(&board),
            &ExportRequest {
                candidate_root: mapping_root.clone(),
                candidate_label: "negative".to_owned(),
                variant: "default".to_owned(),
                include_step: false,
            },
        )
        .unwrap();
        let mut manifest = mapping_candidate.manifest;
        let front = manifest
            .outputs
            .iter_mut()
            .find(|entry| entry.role == "gerber-copper-front")
            .unwrap();
        front.path = "fabrication/board-B_Cu.gbr".to_owned();
        fs::write(
            mapping_candidate.directory.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let mapping_verification = verify_candidate(&mapping_candidate.directory, &board, &profile);
        assert!(mapping_verification.findings.iter().any(|finding| {
            finding.code == "package.wrong-layer-file-mapping" && finding.blocking
        }));
        fs::remove_dir_all(mapping_root).unwrap();

        let cpl_root = temporary_root("wrong-cpl-convention");
        let cpl_candidate = export_candidate(
            &board,
            &profile,
            &run_native_drc(&board),
            &ExportRequest {
                candidate_root: cpl_root.clone(),
                candidate_label: "negative".to_owned(),
                variant: "default".to_owned(),
                include_step: false,
            },
        )
        .unwrap();
        fs::write(
            cpl_candidate.directory.join("assembly/cpl.csv"),
            "Designator,Mid X,Mid Y,Layer,Rotation,Unit,Origin\nX1,0,0,Inner,999,inch,center\n",
        )
        .unwrap();
        let cpl_verification = verify_candidate(&cpl_candidate.directory, &board, &profile);
        assert!(cpl_verification.findings.iter().any(|finding| {
            finding.code == "package.cpl-coordinate-convention" && finding.blocking
        }));
        fs::remove_dir_all(cpl_root).unwrap();
    }
}
