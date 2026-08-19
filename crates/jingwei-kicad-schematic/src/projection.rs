use std::collections::{BTreeMap, BTreeSet};

use jingwei_schematic::{PinElectricalType, SchematicGraph};
use sha2::{Digest, Sha256};

use crate::{
    KICAD_PROJECTION_SCHEMA_VERSION, KICAD_TARGET_VERSION, KicadNetProjection, KicadObjectMapEntry,
    KicadPinProjection, KicadProjectionModel, KicadSymbolProjection, ProjectionCoverage,
    ProjectionError,
};

/// Projects the logical graph into a deterministic, versioned reference model.
pub fn project_schematic(graph: &SchematicGraph) -> Result<KicadProjectionModel, ProjectionError> {
    graph
        .validate()
        .map_err(|error| ProjectionError::InvalidGraph(error.to_string()))?;
    let root_uuid = stable_uuid(graph.id.as_str());
    let sheets = graph
        .sheets
        .values()
        .map(|sheet| {
            (
                stable_uuid(sheet.id.as_str()),
                sheet.hierarchical_path.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut object_map = vec![map_entry(graph.id.as_str(), &root_uuid, "schematic")];
    for sheet in graph.sheets.values() {
        object_map.push(map_entry(
            sheet.id.as_str(),
            &stable_uuid(sheet.id.as_str()),
            "sheet",
        ));
    }
    let mut symbols = Vec::new();
    for symbol in graph.symbols.values() {
        let uuid = stable_uuid(symbol.id.as_str());
        object_map.push(map_entry(symbol.id.as_str(), &uuid, "symbol"));
        let mut pins = Vec::new();
        for pin in symbol.pins.values() {
            pins.push(KicadPinProjection {
                uuid: stable_uuid(pin.id.as_str()),
                jir_entity_id: pin.id.to_string(),
                number: pin.number.clone(),
                name: pin.name.clone(),
                electrical_type: pin_electrical_type(&pin.electrical_type).to_owned(),
                required_connection: pin.required_connection,
                net_uuid: pin
                    .net_id
                    .as_ref()
                    .map(|net_id| stable_uuid(net_id.as_str())),
            });
            object_map.push(map_entry(
                pin.id.as_str(),
                &stable_uuid(pin.id.as_str()),
                "pin",
            ));
        }
        pins.sort_by(|left, right| {
            left.number
                .cmp(&right.number)
                .then_with(|| left.jir_entity_id.cmp(&right.jir_entity_id))
        });
        symbols.push(KicadSymbolProjection {
            uuid,
            jir_entity_id: symbol.id.to_string(),
            sheet_uuid: stable_uuid(symbol.sheet_id.as_str()),
            library_id: symbol.symbol_asset_id.clone(),
            reference: symbol.reference.clone(),
            value: symbol.value.clone(),
            footprint: symbol.footprint_asset_id.clone(),
            x_mm: symbol.position.x,
            y_mm: symbol.position.y,
            rotation_deg: symbol.rotation_deg,
            mirrored: symbol.mirrored,
            pins,
        });
    }
    symbols.sort_by(|left, right| left.jir_entity_id.cmp(&right.jir_entity_id));
    let mut nets = Vec::new();
    for net in graph.nets.values() {
        let uuid = stable_uuid(net.id.as_str());
        object_map.push(map_entry(net.id.as_str(), &uuid, "net"));
        let mut pin_uuids = net
            .pin_ids
            .iter()
            .map(|pin_id| stable_uuid(pin_id.as_str()))
            .collect::<Vec<_>>();
        pin_uuids.sort();
        nets.push(KicadNetProjection {
            uuid,
            jir_entity_id: net.id.to_string(),
            name: net.name.clone(),
            pin_uuids,
        });
    }
    nets.sort_by(|left, right| left.jir_entity_id.cmp(&right.jir_entity_id));
    for wire in graph.wires.values() {
        object_map.push(map_entry(
            wire.id.as_str(),
            &stable_uuid(wire.id.as_str()),
            "wire",
        ));
    }
    for label in graph.labels.values() {
        object_map.push(map_entry(
            label.id.as_str(),
            &stable_uuid(label.id.as_str()),
            "label",
        ));
    }
    object_map.sort_by(|left, right| left.jir_entity_id.cmp(&right.jir_entity_id));
    Ok(KicadProjectionModel {
        schema_version: KICAD_PROJECTION_SCHEMA_VERSION.to_owned(),
        target_kicad_version: KICAD_TARGET_VERSION.to_owned(),
        generator: "jingwei-agent".to_owned(),
        generator_version: env!("CARGO_PKG_VERSION").to_owned(),
        source_graph_id: graph.id.to_string(),
        root_uuid,
        sheets,
        symbols,
        nets,
        object_map,
        coverage: ProjectionCoverage {
            projected_features: vec![
                "hierarchical sheets".to_owned(),
                "symbol instances and stable references".to_owned(),
                "pin membership and logical nets".to_owned(),
                "symbol position, rotation, mirror and footprint reference".to_owned(),
                "JIR-to-external object map".to_owned(),
                "native .kicad_sch symbol, pin, net-label and no-connect serialization".to_owned(),
            ],
            coverage_gaps: vec![
                "explicit wire geometry, buses and graphical annotations remain retained only in JIR".to_owned(),
                "hierarchical sheets are mapped but native export currently emits the root sheet only".to_owned(),
            ],
            round_trip_model_verified: true,
            // A projection cannot certify a tool invocation that has not happened. The
            // external KiCad gate records that evidence separately with its binary hash.
            kicad_cli_verified: false,
        },
    })
}

/// Verifies that a parsed projection is semantically identical to the deterministic
/// projection of the authoritative logical graph.
///
/// This closes the generated model round trip without pretending that arbitrary or
/// unsupported KiCad edits can be imported. Native files with unknown features remain
/// read-only until a dedicated importer can preserve those features losslessly.
pub fn verify_projection_equivalence(
    graph: &SchematicGraph,
    model: &KicadProjectionModel,
) -> Result<(), ProjectionError> {
    validate_projection(model)?;
    let expected = project_schematic(graph)?;
    if expected.source_graph_id != model.source_graph_id
        || expected.root_uuid != model.root_uuid
        || expected.sheets != model.sheets
        || expected.symbols != model.symbols
        || expected.nets != model.nets
        || expected.object_map != model.object_map
    {
        return Err(ProjectionError::InvalidObjectMap(
            "projection does not match the authoritative schematic graph".to_owned(),
        ));
    }
    Ok(())
}

fn pin_electrical_type(value: &PinElectricalType) -> &'static str {
    match value {
        PinElectricalType::Input => "input",
        PinElectricalType::Output => "output",
        PinElectricalType::Bidirectional => "bidirectional",
        PinElectricalType::Passive => "passive",
        PinElectricalType::PowerInput => "power_in",
        PinElectricalType::PowerOutput => "power_out",
        PinElectricalType::OpenCollector => "open_collector",
        PinElectricalType::NoConnect => "no_connect",
    }
}

/// Deterministically serializes the typed projection model.
pub fn serialize_projection(model: &KicadProjectionModel) -> Result<Vec<u8>, ProjectionError> {
    validate_projection(model)?;
    serde_json::to_vec_pretty(model).map_err(Into::into)
}

/// Parses and validates a projection-model round trip.
pub fn parse_projection(bytes: &[u8]) -> Result<KicadProjectionModel, ProjectionError> {
    let model: KicadProjectionModel = serde_json::from_slice(bytes)?;
    validate_projection(&model)?;
    Ok(model)
}

pub(crate) fn validate_projection(model: &KicadProjectionModel) -> Result<(), ProjectionError> {
    if model.schema_version != KICAD_PROJECTION_SCHEMA_VERSION {
        return Err(ProjectionError::UnsupportedVersion(
            model.schema_version.clone(),
        ));
    }
    let mut jir_ids = BTreeSet::new();
    let mut external_ids = BTreeSet::new();
    for entry in &model.object_map {
        if !jir_ids.insert(&entry.jir_entity_id) {
            return Err(ProjectionError::InvalidObjectMap(format!(
                "duplicate JIR entity {}",
                entry.jir_entity_id
            )));
        }
        if !external_ids.insert(&entry.kicad_uuid) {
            return Err(ProjectionError::InvalidObjectMap(format!(
                "duplicate external UUID {}",
                entry.kicad_uuid
            )));
        }
    }
    Ok(())
}

fn map_entry(jir_entity_id: &str, uuid: &str, kind: &str) -> KicadObjectMapEntry {
    KicadObjectMapEntry {
        jir_entity_id: jir_entity_id.to_owned(),
        kicad_uuid: uuid.to_owned(),
        object_kind: kind.to_owned(),
    }
}

fn stable_uuid(value: &str) -> String {
    let digest = Sha256::digest(format!("jingwei:kicad-schematic:{value}").as_bytes());
    let hex = format!("{digest:x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use jingwei_jir::EntityId;
    use jingwei_schematic::{SchematicGraph, Sheet};

    use super::*;

    #[test]
    fn typed_projection_round_trips_without_text_replacement() {
        let sheet_id = EntityId::new("sheet:root").expect("id");
        let graph = SchematicGraph {
            id: EntityId::new("schematic:projection-test").expect("id"),
            schema_version: "schematic-domain/0.1".to_owned(),
            sheets: BTreeMap::from([(
                sheet_id.clone(),
                Sheet {
                    id: sheet_id,
                    parent_sheet_id: None,
                    name: "Root".to_owned(),
                    hierarchical_path: "/".to_owned(),
                },
            )]),
            ..SchematicGraph::default()
        };
        let projected = project_schematic(&graph).expect("project");
        let bytes = serialize_projection(&projected).expect("serialize");
        let reparsed = parse_projection(&bytes).expect("parse");
        assert_eq!(projected, reparsed);
        assert_eq!(reparsed.target_kicad_version, "10.0.5");
        assert!(reparsed.coverage.round_trip_model_verified);
        assert!(!reparsed.coverage.kicad_cli_verified);
        verify_projection_equivalence(&graph, &reparsed).expect("semantic equivalence");

        let mut altered = reparsed;
        altered.sheets.clear();
        assert!(verify_projection_equivalence(&graph, &altered).is_err());
    }
}
