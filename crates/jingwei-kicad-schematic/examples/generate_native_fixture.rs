use std::{collections::BTreeMap, env, fs, path::PathBuf};

use jingwei_jir::{EditControl, EntityId};
use jingwei_kicad_schematic::{
    KICAD_NATIVE_SYMBOL_LIBRARY_FILE, build_native_artifact, project_schematic,
    serialize_projection,
};
use jingwei_schematic::{
    Net, NetClass, PinElectricalType, PinInstance, PointMm, PowerRole, SchematicGraph, Sheet,
    SymbolInstance,
};
use serde_json::to_vec_pretty;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_dir = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/logical/reports/kicad-native"));
    fs::create_dir_all(&output_dir)?;

    let graph = fixture_graph()?;
    let projection = project_schematic(&graph)?;
    let artifact = build_native_artifact(&projection)?;
    fs::write(
        output_dir.join("jingwei-native-reference.kicad_sch"),
        &artifact.schematic,
    )?;
    fs::write(
        output_dir.join("jingwei-native-reference.object-map.json"),
        &artifact.object_map,
    )?;
    fs::write(
        output_dir.join(KICAD_NATIVE_SYMBOL_LIBRARY_FILE),
        &artifact.symbol_library,
    )?;
    fs::write(
        output_dir.join("sym-lib-table"),
        &artifact.symbol_library_table,
    )?;
    fs::write(
        output_dir.join("jingwei-native-reference.kicad_pro"),
        b"{}\n",
    )?;
    fs::write(
        output_dir.join("jingwei-native-reference.projection.json"),
        serialize_projection(&projection)?,
    )?;
    fs::write(
        output_dir.join("jingwei-native-reference.summary.json"),
        to_vec_pretty(&artifact.summary)?,
    )?;
    println!(
        "{}",
        to_vec_pretty(&artifact.summary).map(String::from_utf8)??
    );
    Ok(())
}

fn fixture_graph() -> Result<SchematicGraph, Box<dyn std::error::Error>> {
    let sheet_id = id("sheet:root")?;
    let source_id = id("symbol:source")?;
    let sink_id = id("symbol:sink")?;
    let source_signal_pin = id("pin:source-signal")?;
    let source_ground_pin = id("pin:source-ground")?;
    let sink_signal_pin = id("pin:sink-signal")?;
    let sink_ground_pin = id("pin:sink-ground")?;
    let signal_net = id("net:signal")?;
    let ground_net = id("net:ground")?;

    let source = symbol(
        source_id.clone(),
        sheet_id.clone(),
        "U1",
        "Signal Source",
        PointMm { x: 60.96, y: 50.8 },
        vec![
            pin(
                source_signal_pin.clone(),
                source_id.clone(),
                "1",
                "SIGNAL",
                signal_net.clone(),
            ),
            pin(
                source_ground_pin.clone(),
                source_id.clone(),
                "2",
                "GND",
                ground_net.clone(),
            ),
        ],
    );
    let sink = symbol(
        sink_id.clone(),
        sheet_id.clone(),
        "U2",
        "Signal Sink",
        PointMm { x: 91.44, y: 50.8 },
        vec![
            pin(
                sink_signal_pin.clone(),
                sink_id.clone(),
                "1",
                "SIGNAL",
                signal_net.clone(),
            ),
            pin(
                sink_ground_pin.clone(),
                sink_id.clone(),
                "2",
                "GND",
                ground_net.clone(),
            ),
        ],
    );

    Ok(SchematicGraph {
        id: id("schematic:kicad-native-reference")?,
        schema_version: "schematic-domain/0.1".to_owned(),
        sheets: BTreeMap::from([(
            sheet_id.clone(),
            Sheet {
                id: sheet_id.clone(),
                parent_sheet_id: None,
                name: "Root".to_owned(),
                hierarchical_path: "/".to_owned(),
            },
        )]),
        symbols: BTreeMap::from([(source_id, source), (sink_id, sink)]),
        nets: BTreeMap::from([
            (
                signal_net.clone(),
                Net {
                    id: signal_net,
                    sheet_id: sheet_id.clone(),
                    name: "SIGNAL".to_owned(),
                    class: NetClass::Signal,
                    pin_ids: [source_signal_pin, sink_signal_pin].into_iter().collect(),
                    voltage: None,
                    current_budget_a: None,
                    requires_test_point: false,
                    evidence_refs: vec!["fixture:kicad-native-reference".to_owned()],
                },
            ),
            (
                ground_net.clone(),
                Net {
                    id: ground_net,
                    sheet_id,
                    name: "GND".to_owned(),
                    class: NetClass::Ground,
                    pin_ids: [source_ground_pin, sink_ground_pin].into_iter().collect(),
                    voltage: None,
                    current_budget_a: None,
                    requires_test_point: false,
                    evidence_refs: vec!["fixture:kicad-native-reference".to_owned()],
                },
            ),
        ]),
        ..SchematicGraph::default()
    })
}

fn symbol(
    symbol_id: EntityId,
    sheet_id: EntityId,
    reference: &str,
    value: &str,
    position: PointMm,
    pins: Vec<PinInstance>,
) -> SymbolInstance {
    SymbolInstance {
        id: symbol_id,
        sheet_id,
        module_id: None,
        reference: reference.to_owned(),
        value: value.to_owned(),
        exact_mpn: format!("JW-{reference}-REFERENCE"),
        part_decision_id: format!("decision:{reference}"),
        symbol_asset_id: if reference == "U1" {
            "Connector_Generic:Conn_01x02".to_owned()
        } else {
            "Connector_Generic:Conn_02x01".to_owned()
        },
        footprint_asset_id: None,
        position,
        rotation_deg: 0.0,
        mirrored: false,
        pins: pins.into_iter().map(|pin| (pin.id.clone(), pin)).collect(),
        properties: BTreeMap::new(),
        evidence_refs: vec!["fixture:kicad-native-reference".to_owned()],
        edit_control: EditControl::default(),
    }
}

fn pin(
    pin_id: EntityId,
    symbol_id: EntityId,
    number: &str,
    name: &str,
    net_id: EntityId,
) -> PinInstance {
    PinInstance {
        id: pin_id,
        symbol_id,
        number: number.to_owned(),
        name: name.to_owned(),
        electrical_type: PinElectricalType::Passive,
        power_role: PowerRole::None,
        required_connection: true,
        net_id: Some(net_id),
        allowed_voltage: None,
        power_domain: None,
        interface_id: None,
        properties: BTreeMap::new(),
        evidence_refs: vec!["fixture:kicad-native-reference".to_owned()],
    }
}

fn id(value: &str) -> Result<EntityId, Box<dyn std::error::Error>> {
    EntityId::new(value).map_err(Into::into)
}
