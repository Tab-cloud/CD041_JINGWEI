use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    KICAD_NATIVE_SYMBOL_LIBRARY_FILE, KICAD_TARGET_VERSION, KicadNativeArtifact,
    KicadNativeSummary, KicadNetProjection, KicadProjectionModel, KicadSymbolProjection,
    ProjectionError,
};

const KICAD_FILE_FORMAT_VERSION: &str = "20220904";
const PIN_SPACING_MM: f64 = 2.54;
const PIN_LENGTH_MM: f64 = 2.54;
const PIN_ANCHOR_X_MM: f64 = -5.08;

/// Builds the native schematic and its versioned JIR-to-KiCad object-map sidecar.
pub fn build_native_artifact(
    model: &KicadProjectionModel,
) -> Result<KicadNativeArtifact, ProjectionError> {
    let schematic = serialize_native_schematic(model)?;
    let object_map = serialize_object_map(model)?;
    let symbol_library = serialize_symbol_library(model)?;
    let symbol_library_table = serialize_symbol_library_table();
    let summary = parse_native_schematic(&schematic)?;
    Ok(KicadNativeArtifact {
        schematic,
        object_map,
        symbol_library,
        symbol_library_table,
        summary,
    })
}

/// Serializes a projection into a deterministic, native KiCad schematic.
///
/// KiCad 10.0.5 reads this stable 2022 native schema and upgrades it on save.
/// Symbols are self-contained in `lib_symbols`; named labels placed on pin
/// anchors retain logical net membership without relying on external libraries.
pub fn serialize_native_schematic(
    model: &KicadProjectionModel,
) -> Result<Vec<u8>, ProjectionError> {
    validate_native_model(model)?;
    let placements = pin_placements(model);
    let library_ids = model
        .symbols
        .iter()
        .map(|symbol| (symbol.uuid.clone(), native_library_id(symbol)))
        .collect::<BTreeMap<_, _>>();

    let mut output = String::new();
    output.push_str("(kicad_sch (version ");
    output.push_str(KICAD_FILE_FORMAT_VERSION);
    output.push_str(") (generator jingwei-agent)\n\n");
    output.push_str("  (uuid ");
    output.push_str(&model.root_uuid);
    output.push_str(")\n\n  (paper \"A4\")\n\n  (lib_symbols\n");
    for symbol in &model.symbols {
        write_library_symbol(&mut output, symbol, &library_ids[&symbol.uuid]);
    }
    output.push_str("  )\n\n");

    write_net_labels(&mut output, &model.nets, &placements);
    write_no_connects(&mut output, model, &placements);
    for symbol in &model.symbols {
        write_symbol_instance(&mut output, symbol, &library_ids[&symbol.uuid]);
    }

    output.push_str("  (sheet_instances\n    (path \"/\" (page \"1\"))\n  )\n\n");
    output.push_str("  (symbol_instances\n");
    for symbol in &model.symbols {
        output.push_str("    (path \"/");
        output.push_str(&symbol.uuid);
        output.push_str("\"\n      (reference ");
        push_quoted(&mut output, &symbol.reference);
        output.push_str(") (unit 1) (value ");
        push_quoted(&mut output, &symbol.value);
        output.push_str(") (footprint ");
        push_quoted(&mut output, symbol.footprint.as_deref().unwrap_or(""));
        output.push_str(")\n    )\n");
    }
    output.push_str("  )\n)\n");
    Ok(output.into_bytes())
}

/// Serializes the generated, project-local symbol library used by native output.
pub fn serialize_symbol_library(model: &KicadProjectionModel) -> Result<Vec<u8>, ProjectionError> {
    validate_native_model(model)?;
    let mut output =
        String::from("(kicad_symbol_lib (version 20220914) (generator jingwei-agent)\n");
    for symbol in &model.symbols {
        let native_id = native_library_id(symbol);
        let short_name = native_id
            .split_once(':')
            .map_or(native_id.as_str(), |(_, name)| name);
        write_library_symbol(&mut output, symbol, short_name);
    }
    output.push_str(")\n");
    Ok(output.into_bytes())
}

/// Serializes a project-local `sym-lib-table` for the generated library.
#[must_use]
pub fn serialize_symbol_library_table() -> Vec<u8> {
    format!(
        "(sym_lib_table\n  (version 7)\n  (lib (name \"Jingwei\")(type \"KiCad\")(uri \"${{KIPRJMOD}}/{KICAD_NATIVE_SYMBOL_LIBRARY_FILE}\")(options \"\")(descr \"Jingwei generated native symbols\"))\n)\n"
    )
    .into_bytes()
}

/// Serializes the traceability map as a versioned JSON sidecar.
pub fn serialize_object_map(model: &KicadProjectionModel) -> Result<Vec<u8>, ProjectionError> {
    validate_native_model(model)?;
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ObjectMapDocument<'a> {
        schema_version: &'static str,
        source_graph_id: &'a str,
        target_kicad_version: &'a str,
        entries: &'a [crate::KicadObjectMapEntry],
    }

    serde_json::to_vec_pretty(&ObjectMapDocument {
        schema_version: "jingwei-kicad-object-map/0.1",
        source_graph_id: &model.source_graph_id,
        target_kicad_version: &model.target_kicad_version,
        entries: &model.object_map,
    })
    .map_err(Into::into)
}

/// Parses native KiCad S-expression syntax and returns structural metadata.
///
/// This parser intentionally validates structure rather than using textual
/// replacement or regular expressions. It accepts both generated files and
/// files upgraded by KiCad 10.0.5.
pub fn parse_native_schematic(bytes: &[u8]) -> Result<KicadNativeSummary, ProjectionError> {
    let mut parser = SExprParser::new(bytes);
    let root = parser.parse_document()?;
    let root_items = root.as_list().ok_or_else(|| {
        ProjectionError::InvalidNativeSyntax("document root must be a list".to_owned())
    })?;
    if list_head(root_items) != Some("kicad_sch") {
        return Err(ProjectionError::InvalidNativeSyntax(
            "document root must be kicad_sch".to_owned(),
        ));
    }

    let format_version = required_child_value(root_items, "version")?;
    let generator = required_child_value(root_items, "generator")?;
    let root_uuid = required_child_value(root_items, "uuid")?;
    let library_symbol_count = direct_child(root_items, "lib_symbols")
        .map(|items| count_direct_children(items, "symbol"))
        .unwrap_or(0);
    let symbol_lists = direct_children(root_items, "symbol");
    let pin_count = symbol_lists
        .iter()
        .map(|items| count_direct_children(items, "pin"))
        .sum();
    let label_count = ["label", "global_label", "hierarchical_label"]
        .iter()
        .map(|head| count_direct_children(root_items, head))
        .sum();

    Ok(KicadNativeSummary {
        format_version,
        generator,
        root_uuid,
        library_symbol_count,
        symbol_count: symbol_lists.len(),
        pin_count,
        label_count,
        no_connect_count: count_direct_children(root_items, "no_connect"),
    })
}

#[derive(Clone, Copy)]
struct PinPlacement {
    x: f64,
    y: f64,
}

fn pin_placements(model: &KicadProjectionModel) -> BTreeMap<String, PinPlacement> {
    let mut placements = BTreeMap::new();
    for symbol in &model.symbols {
        for (index, pin) in symbol.pins.iter().enumerate() {
            let local_y = pin_local_y(index, symbol.pins.len());
            let (x, y) = transform_pin_anchor(symbol, local_y);
            placements.insert(pin.uuid.clone(), PinPlacement { x, y });
        }
    }
    placements
}

fn transform_pin_anchor(symbol: &KicadSymbolProjection, local_y: f64) -> (f64, f64) {
    let mut local_x = PIN_ANCHOR_X_MM;
    if symbol.mirrored {
        local_x = -local_x;
    }
    let radians = symbol.rotation_deg.to_radians();
    let rotated_x = local_x * radians.cos() - local_y * radians.sin();
    let rotated_y = local_x * radians.sin() + local_y * radians.cos();
    (symbol.x_mm + rotated_x, symbol.y_mm + rotated_y)
}

fn write_library_symbol(output: &mut String, symbol: &KicadSymbolProjection, lib_id: &str) {
    let short_name = lib_id.split_once(':').map_or(lib_id, |(_, name)| name);
    let half_height = symbol_half_height(symbol.pins.len());
    output.push_str("    (symbol ");
    push_quoted(output, lib_id);
    output.push_str(" (pin_names (offset 1.016)) (in_bom yes) (on_board yes)\n");
    write_library_property(
        output,
        "Reference",
        reference_prefix(&symbol.reference),
        0,
        -half_height - 2.54,
        false,
    );
    write_library_property(output, "Value", &symbol.value, 1, half_height + 2.54, false);
    write_library_property(
        output,
        "Footprint",
        symbol.footprint.as_deref().unwrap_or(""),
        2,
        0.0,
        true,
    );
    write_library_property(output, "Datasheet", "~", 3, 0.0, true);

    output.push_str("      (symbol ");
    push_quoted(output, &format!("{short_name}_0_1"));
    output.push_str("\n        (rectangle (start -2.54 ");
    output.push_str(&format_number(-half_height));
    output.push_str(") (end 2.54 ");
    output.push_str(&format_number(half_height));
    output.push_str(")\n          (stroke (width 0.254) (type default))\n          (fill (type background))\n        )\n      )\n");

    output.push_str("      (symbol ");
    push_quoted(output, &format!("{short_name}_1_1"));
    output.push('\n');
    for (index, pin) in symbol.pins.iter().enumerate() {
        output.push_str("        (pin ");
        output.push_str(&pin.electrical_type);
        output.push_str(" line (at -5.08 ");
        output.push_str(&format_number(pin_local_y(index, symbol.pins.len())));
        output.push_str(" 0) (length ");
        output.push_str(&format_number(PIN_LENGTH_MM));
        output.push_str(")\n          (name ");
        push_quoted(output, display_pin_name(&pin.name));
        output.push_str(" (effects (font (size 1.27 1.27))))\n          (number ");
        push_quoted(output, &pin.number);
        output.push_str(" (effects (font (size 1.27 1.27))))\n        )\n");
    }
    output.push_str("      )\n    )\n");
}

fn write_library_property(
    output: &mut String,
    name: &str,
    value: &str,
    id: usize,
    y: f64,
    hidden: bool,
) {
    output.push_str("      (property ");
    push_quoted(output, name);
    output.push(' ');
    push_quoted(output, value);
    output.push_str(" (id ");
    output.push_str(&id.to_string());
    output.push_str(") (at 0 ");
    output.push_str(&format_number(y));
    output.push_str(" 0)\n        (effects (font (size 1.27 1.27))");
    if hidden {
        output.push_str(" hide");
    }
    output.push_str(")\n      )\n");
}

fn write_net_labels(
    output: &mut String,
    nets: &[KicadNetProjection],
    placements: &BTreeMap<String, PinPlacement>,
) {
    for (net_index, net) in nets.iter().enumerate() {
        if net.pin_uuids.is_empty() {
            write_label(
                output,
                &net.name,
                20.0 + net_index as f64 * 5.08,
                20.0,
                &net.uuid,
            );
            continue;
        }
        for (index, pin_uuid) in net.pin_uuids.iter().enumerate() {
            let placement = placements[pin_uuid];
            let label_uuid = if index == 0 {
                net.uuid.clone()
            } else {
                derived_uuid(&format!("net-label:{}:{pin_uuid}", net.jir_entity_id))
            };
            write_label(output, &net.name, placement.x, placement.y, &label_uuid);
        }
    }
}

fn write_label(output: &mut String, name: &str, x: f64, y: f64, uuid: &str) {
    output.push_str("  (label ");
    push_quoted(output, name);
    output.push_str(" (at ");
    output.push_str(&format_number(x));
    output.push(' ');
    output.push_str(&format_number(y));
    output.push_str(" 0)\n    (effects (font (size 1.27 1.27)) (justify left bottom))\n    (uuid ");
    output.push_str(uuid);
    output.push_str(")\n  )\n\n");
}

fn write_no_connects(
    output: &mut String,
    model: &KicadProjectionModel,
    placements: &BTreeMap<String, PinPlacement>,
) {
    for symbol in &model.symbols {
        for pin in &symbol.pins {
            if pin.net_uuid.is_none()
                && (!pin.required_connection || pin.electrical_type == "no_connect")
            {
                let placement = placements[&pin.uuid];
                output.push_str("  (no_connect (at ");
                output.push_str(&format_number(placement.x));
                output.push(' ');
                output.push_str(&format_number(placement.y));
                output.push_str(") (uuid ");
                output.push_str(&derived_uuid(&format!("no-connect:{}", pin.jir_entity_id)));
                output.push_str("))\n\n");
            }
        }
    }
}

fn write_symbol_instance(output: &mut String, symbol: &KicadSymbolProjection, lib_id: &str) {
    output.push_str("  (symbol (lib_id ");
    push_quoted(output, lib_id);
    output.push_str(") (at ");
    output.push_str(&format_number(symbol.x_mm));
    output.push(' ');
    output.push_str(&format_number(symbol.y_mm));
    output.push(' ');
    output.push_str(&format_number(normalized_rotation(symbol.rotation_deg)));
    output.push(')');
    if symbol.mirrored {
        output.push_str(" (mirror y)");
    }
    output.push_str(" (unit 1)\n    (in_bom yes) (on_board yes)\n    (uuid ");
    output.push_str(&symbol.uuid);
    output.push_str(")\n");
    write_instance_property(
        output,
        "Reference",
        &symbol.reference,
        0,
        symbol.x_mm,
        symbol.y_mm - 5.08,
        false,
    );
    write_instance_property(
        output,
        "Value",
        &symbol.value,
        1,
        symbol.x_mm,
        symbol.y_mm + 5.08,
        false,
    );
    write_instance_property(
        output,
        "Footprint",
        symbol.footprint.as_deref().unwrap_or(""),
        2,
        symbol.x_mm,
        symbol.y_mm,
        true,
    );
    write_instance_property(output, "Datasheet", "~", 3, symbol.x_mm, symbol.y_mm, true);
    write_instance_property(
        output,
        "JingweiJirId",
        &symbol.jir_entity_id,
        4,
        symbol.x_mm,
        symbol.y_mm,
        true,
    );
    write_instance_property(
        output,
        "JingweiSourceLibraryId",
        &symbol.library_id,
        5,
        symbol.x_mm,
        symbol.y_mm,
        true,
    );
    for pin in &symbol.pins {
        output.push_str("    (pin ");
        push_quoted(output, &pin.number);
        output.push_str(" (uuid ");
        output.push_str(&pin.uuid);
        output.push_str("))\n");
    }
    output.push_str("  )\n\n");
}

fn write_instance_property(
    output: &mut String,
    name: &str,
    value: &str,
    id: usize,
    x: f64,
    y: f64,
    hidden: bool,
) {
    output.push_str("    (property ");
    push_quoted(output, name);
    output.push(' ');
    push_quoted(output, value);
    output.push_str(" (id ");
    output.push_str(&id.to_string());
    output.push_str(") (at ");
    output.push_str(&format_number(x));
    output.push(' ');
    output.push_str(&format_number(y));
    output.push_str(" 0)\n      (effects (font (size 1.27 1.27))");
    if hidden {
        output.push_str(" hide");
    }
    output.push_str(")\n    )\n");
}

fn validate_native_model(model: &KicadProjectionModel) -> Result<(), ProjectionError> {
    if model.target_kicad_version != KICAD_TARGET_VERSION {
        return Err(ProjectionError::InvalidNativeData(format!(
            "target KiCad version must be {KICAD_TARGET_VERSION}, found {}",
            model.target_kicad_version
        )));
    }
    let mut references = BTreeSet::new();
    let mut pin_uuids = BTreeSet::new();
    let mut pins_by_uuid = BTreeMap::new();
    for symbol in &model.symbols {
        if !references.insert(&symbol.reference) {
            return Err(ProjectionError::InvalidNativeData(format!(
                "duplicate symbol reference {}",
                symbol.reference
            )));
        }
        if symbol.library_id.trim().is_empty() {
            return Err(ProjectionError::InvalidNativeData(format!(
                "symbol {} has an empty library ID",
                symbol.reference
            )));
        }
        if !symbol.x_mm.is_finite() || !symbol.y_mm.is_finite() {
            return Err(ProjectionError::InvalidNativeData(format!(
                "symbol {} has a non-finite position",
                symbol.reference
            )));
        }
        if !is_quarter_turn(symbol.rotation_deg) {
            return Err(ProjectionError::InvalidNativeData(format!(
                "symbol {} rotation must be a multiple of 90 degrees",
                symbol.reference
            )));
        }
        for pin in &symbol.pins {
            if !pin_uuids.insert(&pin.uuid) {
                return Err(ProjectionError::InvalidNativeData(format!(
                    "duplicate pin UUID {}",
                    pin.uuid
                )));
            }
            validate_electrical_type(&pin.electrical_type)?;
            pins_by_uuid.insert(pin.uuid.as_str(), pin);
        }
    }

    let net_uuids = model
        .nets
        .iter()
        .map(|net| net.uuid.as_str())
        .collect::<BTreeSet<_>>();
    for net in &model.nets {
        for pin_uuid in &net.pin_uuids {
            let pin = pins_by_uuid.get(pin_uuid.as_str()).ok_or_else(|| {
                ProjectionError::InvalidNativeData(format!(
                    "net {} references unknown pin UUID {pin_uuid}",
                    net.name
                ))
            })?;
            if pin.net_uuid.as_deref() != Some(net.uuid.as_str()) {
                return Err(ProjectionError::InvalidNativeData(format!(
                    "pin {} and net {} membership disagree",
                    pin.jir_entity_id, net.jir_entity_id
                )));
            }
        }
    }
    for pin in pins_by_uuid.values() {
        if let Some(net_uuid) = pin.net_uuid.as_deref()
            && !net_uuids.contains(net_uuid)
        {
            return Err(ProjectionError::InvalidNativeData(format!(
                "pin {} references unknown net UUID {net_uuid}",
                pin.jir_entity_id
            )));
        }
    }
    Ok(())
}

fn validate_electrical_type(value: &str) -> Result<(), ProjectionError> {
    const TYPES: [&str; 8] = [
        "input",
        "output",
        "bidirectional",
        "passive",
        "power_in",
        "power_out",
        "open_collector",
        "no_connect",
    ];
    if TYPES.contains(&value) {
        Ok(())
    } else {
        Err(ProjectionError::InvalidNativeData(format!(
            "unsupported pin electrical type {value}"
        )))
    }
}

fn native_library_id(symbol: &KicadSymbolProjection) -> String {
    let sanitized = symbol
        .reference
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("Jingwei:JW_{sanitized}_{}", &symbol.uuid[..8])
}

fn reference_prefix(reference: &str) -> &str {
    let end = reference
        .char_indices()
        .take_while(|(_, value)| value.is_ascii_alphabetic())
        .map(|(index, value)| index + value.len_utf8())
        .last()
        .unwrap_or(0);
    if end == 0 { "U" } else { &reference[..end] }
}

fn display_pin_name(name: &str) -> &str {
    if name.is_empty() { "~" } else { name }
}

fn pin_local_y(index: usize, pin_count: usize) -> f64 {
    (index as f64 - (pin_count.saturating_sub(1)) as f64 / 2.0) * PIN_SPACING_MM
}

fn symbol_half_height(pin_count: usize) -> f64 {
    ((pin_count.saturating_sub(1)) as f64 * PIN_SPACING_MM + 5.08) / 2.0
}

fn normalized_rotation(value: f64) -> f64 {
    value.rem_euclid(360.0)
}

fn is_quarter_turn(value: f64) -> bool {
    value.is_finite() && (value / 90.0 - (value / 90.0).round()).abs() < 1e-9
}

fn format_number(value: f64) -> String {
    let normalized = if value.abs() < 0.000_000_5 {
        0.0
    } else {
        value
    };
    if (normalized - normalized.round()).abs() < 0.000_000_5 {
        return format!("{normalized:.0}");
    }
    let formatted = format!("{normalized:.6}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn push_quoted(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            value => output.push(value),
        }
    }
    output.push('"');
}

fn derived_uuid(value: &str) -> String {
    let digest = Sha256::digest(format!("jingwei:kicad-native:{value}").as_bytes());
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum SExpr {
    Atom(String),
    String(String),
    List(Vec<SExpr>),
}

impl SExpr {
    fn as_text(&self) -> Option<&str> {
        match self {
            Self::Atom(value) | Self::String(value) => Some(value),
            Self::List(_) => None,
        }
    }

    fn as_list(&self) -> Option<&[Self]> {
        match self {
            Self::List(items) => Some(items),
            Self::Atom(_) | Self::String(_) => None,
        }
    }
}

struct SExprParser<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> SExprParser<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, position: 0 }
    }

    fn parse_document(&mut self) -> Result<SExpr, ProjectionError> {
        self.skip_whitespace();
        let value = self.parse_expr()?;
        self.skip_whitespace();
        if self.position != self.input.len() {
            return Err(self.syntax_error("unexpected content after document root"));
        }
        Ok(value)
    }

    fn parse_expr(&mut self) -> Result<SExpr, ProjectionError> {
        match self.peek() {
            Some(b'(') => self.parse_list(),
            Some(b'"') => self.parse_string(),
            Some(b')') => Err(self.syntax_error("unexpected closing parenthesis")),
            Some(_) => self.parse_atom(),
            None => Err(self.syntax_error("unexpected end of input")),
        }
    }

    fn parse_list(&mut self) -> Result<SExpr, ProjectionError> {
        self.position += 1;
        let mut items = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b')') => {
                    self.position += 1;
                    return Ok(SExpr::List(items));
                }
                Some(_) => items.push(self.parse_expr()?),
                None => return Err(self.syntax_error("unterminated list")),
            }
        }
    }

    fn parse_string(&mut self) -> Result<SExpr, ProjectionError> {
        self.position += 1;
        let mut bytes = Vec::new();
        loop {
            let next = self
                .next()
                .ok_or_else(|| self.syntax_error("unterminated string"))?;
            match next {
                b'"' => {
                    let value = String::from_utf8(bytes)
                        .map_err(|_| self.syntax_error("quoted string contains invalid UTF-8"))?;
                    return Ok(SExpr::String(value));
                }
                b'\\' => {
                    let escaped = self
                        .next()
                        .ok_or_else(|| self.syntax_error("unterminated escape sequence"))?;
                    match escaped {
                        b'n' => bytes.push(b'\n'),
                        b'r' => bytes.push(b'\r'),
                        b't' => bytes.push(b'\t'),
                        b'\\' | b'"' => bytes.push(escaped),
                        value => bytes.push(value),
                    }
                }
                value => bytes.push(value),
            }
        }
    }

    fn parse_atom(&mut self) -> Result<SExpr, ProjectionError> {
        let start = self.position;
        while let Some(value) = self.peek() {
            if value.is_ascii_whitespace() || matches!(value, b'(' | b')') {
                break;
            }
            self.position += 1;
        }
        let value = String::from_utf8(self.input[start..self.position].to_vec())
            .map_err(|_| self.syntax_error("atom contains invalid UTF-8"))?;
        if value.is_empty() {
            return Err(self.syntax_error("empty atom"));
        }
        Ok(SExpr::Atom(value))
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(|value| value.is_ascii_whitespace()) {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.position).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let value = self.peek()?;
        self.position += 1;
        Some(value)
    }

    fn syntax_error(&self, message: &str) -> ProjectionError {
        ProjectionError::InvalidNativeSyntax(format!("{message} at byte {}", self.position))
    }
}

fn list_head(items: &[SExpr]) -> Option<&str> {
    items.first().and_then(SExpr::as_text)
}

fn direct_children<'a>(items: &'a [SExpr], head: &str) -> Vec<&'a [SExpr]> {
    items
        .iter()
        .filter_map(|item| {
            let child = item.as_list()?;
            (list_head(child) == Some(head)).then_some(child)
        })
        .collect()
}

fn direct_child<'a>(items: &'a [SExpr], head: &str) -> Option<&'a [SExpr]> {
    direct_children(items, head).into_iter().next()
}

fn count_direct_children(items: &[SExpr], head: &str) -> usize {
    direct_children(items, head).len()
}

fn required_child_value(items: &[SExpr], head: &str) -> Result<String, ProjectionError> {
    direct_child(items, head)
        .and_then(|child| child.get(1))
        .and_then(SExpr::as_text)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            ProjectionError::InvalidNativeSyntax(format!(
                "required {head} field is missing or malformed"
            ))
        })
}

#[cfg(test)]
mod tests {
    use crate::{
        KicadObjectMapEntry, KicadPinProjection, KicadProjectionModel, KicadSymbolProjection,
        ProjectionCoverage,
    };

    use super::*;

    #[test]
    fn native_artifact_is_deterministic_and_structurally_parseable() {
        let model = connected_model();
        let first = build_native_artifact(&model).expect("first artifact");
        let second = build_native_artifact(&model).expect("second artifact");
        assert_eq!(first, second);
        assert_eq!(first.summary.format_version, KICAD_FILE_FORMAT_VERSION);
        assert_eq!(first.summary.generator, "jingwei-agent");
        assert_eq!(first.summary.library_symbol_count, 2);
        assert_eq!(first.summary.symbol_count, 2);
        assert_eq!(first.summary.pin_count, 2);
        assert_eq!(first.summary.label_count, 2);
        assert_eq!(first.summary.no_connect_count, 0);
        assert!(
            String::from_utf8(first.schematic)
                .expect("UTF-8")
                .contains("JingweiJirId")
        );
        assert!(
            String::from_utf8(first.object_map)
                .expect("UTF-8")
                .contains("net:signal")
        );
    }

    #[test]
    fn optional_unconnected_pin_gets_native_no_connect_marker() {
        let mut model = connected_model();
        model.nets.clear();
        model.symbols.truncate(1);
        model.symbols[0].pins[0].net_uuid = None;
        model.symbols[0].pins[0].required_connection = false;
        let bytes = serialize_native_schematic(&model).expect("serialize");
        let summary = parse_native_schematic(&bytes).expect("parse");
        assert_eq!(summary.no_connect_count, 1);
        assert_eq!(summary.label_count, 0);
    }

    #[test]
    fn unsupported_symbol_rotation_is_rejected_without_silent_rounding() {
        let mut model = connected_model();
        model.symbols[0].rotation_deg = 45.0;
        let error = serialize_native_schematic(&model).expect_err("rotation must fail");
        assert!(error.to_string().contains("multiple of 90"));
    }

    #[test]
    fn parser_rejects_trailing_or_unbalanced_s_expressions() {
        assert!(parse_native_schematic(b"(kicad_sch (version 1)) extra").is_err());
        assert!(parse_native_schematic(b"(kicad_sch (version 1)").is_err());
    }

    fn connected_model() -> KicadProjectionModel {
        let net_uuid = uuid(9);
        let first_pin_uuid = uuid(3);
        let second_pin_uuid = uuid(5);
        let symbols = vec![
            symbol(
                "symbol:source",
                "U1",
                uuid(2),
                first_pin_uuid.clone(),
                &net_uuid,
                60.0,
            ),
            symbol(
                "symbol:sink",
                "U2",
                uuid(4),
                second_pin_uuid.clone(),
                &net_uuid,
                90.0,
            ),
        ];
        KicadProjectionModel {
            schema_version: crate::KICAD_PROJECTION_SCHEMA_VERSION.to_owned(),
            target_kicad_version: KICAD_TARGET_VERSION.to_owned(),
            generator: "jingwei-agent".to_owned(),
            generator_version: "0.1.0".to_owned(),
            source_graph_id: "schematic:native-test".to_owned(),
            root_uuid: uuid(1),
            sheets: BTreeMap::new(),
            symbols,
            nets: vec![KicadNetProjection {
                uuid: net_uuid.clone(),
                jir_entity_id: "net:signal".to_owned(),
                name: "SIGNAL\"A".to_owned(),
                pin_uuids: vec![first_pin_uuid.clone(), second_pin_uuid.clone()],
            }],
            object_map: vec![
                object_map("schematic:native-test", uuid(1), "schematic"),
                object_map("symbol:source", uuid(2), "symbol"),
                object_map("pin:source", first_pin_uuid, "pin"),
                object_map("symbol:sink", uuid(4), "symbol"),
                object_map("pin:sink", second_pin_uuid, "pin"),
                object_map("net:signal", net_uuid, "net"),
            ],
            coverage: ProjectionCoverage {
                projected_features: Vec::new(),
                coverage_gaps: Vec::new(),
                round_trip_model_verified: true,
                kicad_cli_verified: true,
            },
        }
    }

    fn symbol(
        jir_id: &str,
        reference: &str,
        symbol_uuid: String,
        pin_uuid: String,
        net_uuid: &str,
        x_mm: f64,
    ) -> KicadSymbolProjection {
        KicadSymbolProjection {
            uuid: symbol_uuid,
            jir_entity_id: jir_id.to_owned(),
            sheet_uuid: uuid(8),
            library_id: format!(
                "Connector_Generic:Conn_01x0{}",
                if reference == "U1" { 1 } else { 2 }
            ),
            reference: reference.to_owned(),
            value: format!("Device {reference}"),
            footprint: Some("Package_QFP:LQFP-48".to_owned()),
            x_mm,
            y_mm: 50.0,
            rotation_deg: 0.0,
            mirrored: false,
            pins: vec![KicadPinProjection {
                uuid: pin_uuid,
                jir_entity_id: format!("pin:{}", reference.to_ascii_lowercase()),
                number: "1".to_owned(),
                name: "IO".to_owned(),
                electrical_type: "passive".to_owned(),
                required_connection: true,
                net_uuid: Some(net_uuid.to_owned()),
            }],
        }
    }

    fn object_map(jir_id: &str, kicad_uuid: String, kind: &str) -> KicadObjectMapEntry {
        KicadObjectMapEntry {
            jir_entity_id: jir_id.to_owned(),
            kicad_uuid,
            object_kind: kind.to_owned(),
        }
    }

    fn uuid(value: u8) -> String {
        format!("00000000-0000-0000-0000-{value:012}")
    }
}
