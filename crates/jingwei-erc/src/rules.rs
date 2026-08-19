use std::collections::{BTreeMap, BTreeSet};

use jingwei_components::{Finding, FindingCoverage, Severity, SuggestedAction};
use jingwei_schematic::{PinElectricalType, PowerRole, SchematicGraph};

use crate::{ErcContext, LibraryBindingStatus, RuleCoverage};

const VERSION: &str = "1.0.0";

pub(crate) fn run_all(graph: &SchematicGraph, context: &ErcContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    unconnected_required_pins(graph, &mut findings);
    driver_conflicts(graph, &mut findings);
    power_inputs_without_source(graph, &mut findings);
    voltage_incompatibility(graph, &mut findings);
    footprint_bindings(graph, context, &mut findings);
    duplicate_i2c_addresses(graph, &mut findings);
    decoupling_coverage(graph, &mut findings);
    connector_contracts(graph, &mut findings);
    pin_mux_and_domains(graph, &mut findings);
    dft_coverage(graph, &mut findings);
    no_connect_conflicts(graph, &mut findings);
    assertion_margins(graph, &mut findings);
    findings
}

/// Versioned rule and gap manifest.
#[must_use]
pub fn rule_manifest() -> Vec<RuleCoverage> {
    vec![
        covered(
            "ERC.UNCONNECTED_REQUIRED",
            "Unconnected required pin",
            &["connected/no-connect/unknown pin state"],
            &[],
        ),
        covered(
            "ERC.DRIVER_CONFLICT",
            "Output-output driver conflict",
            &["push-pull outputs on one net"],
            &["tri-state timing analysis"],
        ),
        covered(
            "ERC.POWER_NO_SOURCE",
            "Power input without source",
            &["power input and explicit source roles"],
            &["dynamic sequencing"],
        ),
        covered(
            "ERC.VOLTAGE_RANGE",
            "Voltage and absolute-rating compatibility",
            &["net voltage range inside pin allowed range"],
            &["transients and analog simulation"],
        ),
        covered(
            "ERC.FOOTPRINT_BINDING",
            "Missing or incorrect footprint",
            &["exact verified library binding status"],
            &["physical DRC"],
        ),
        covered(
            "ERC.I2C_ADDRESS",
            "Duplicate I2C address",
            &["bus and normalized address collision"],
            &["run-time address reconfiguration"],
        ),
        covered(
            "ERC.DECOUPLING",
            "Decoupling coverage",
            &["required flag, covered flag and configured value"],
            &["physical distance is deferred to PCB validation"],
        ),
        covered(
            "ERC.CONNECTOR_CONTRACT",
            "Connector interface, protection and direction",
            &["declared interface, direction and protection flags"],
            &["EMC performance"],
        ),
        covered(
            "ERC.PINMUX_DOMAIN",
            "Pin mux, power domain and level shift",
            &["allowed mux and declared domain compatibility"],
            &["firmware build correlation"],
        ),
        covered(
            "ERC.DFT_COVERAGE",
            "Minimum test-point coverage",
            &["nets explicitly requiring test points"],
            &["probe accessibility is deferred to PCB validation"],
        ),
        covered(
            "ERC.NO_CONNECT_CONFLICT",
            "No-connect conflicts",
            &["no-connect marker versus actual connectivity"],
            &[],
        ),
        covered(
            "ERC.ASSERTION_MARGIN",
            "Critical assertion margin",
            &["structured minimum and actual margins"],
            &["simulation-derived margins"],
        ),
        covered(
            "COVERAGE.SI",
            "Signal-integrity analysis",
            &[],
            &["not implemented; required for unsupported high-speed designs"],
        ),
        covered(
            "COVERAGE.THERMAL",
            "Thermal simulation",
            &[],
            &["not implemented; deterministic warning remains a release gap"],
        ),
        covered(
            "COVERAGE.EMC",
            "EMC validation",
            &[],
            &["not implemented; prototype or laboratory evidence is required"],
        ),
    ]
}

fn covered(id: &str, title: &str, checked: &[&str], gaps: &[&str]) -> RuleCoverage {
    RuleCoverage {
        rule_id: id.to_owned(),
        version: VERSION.to_owned(),
        title: title.to_owned(),
        implemented: !checked.is_empty(),
        checked_aspects: checked.iter().map(|value| (*value).to_owned()).collect(),
        gaps: gaps.iter().map(|value| (*value).to_owned()).collect(),
    }
}

fn unconnected_required_pins(graph: &SchematicGraph, findings: &mut Vec<Finding>) {
    let marked = graph
        .no_connects
        .values()
        .map(|value| &value.pin_id)
        .collect::<BTreeSet<_>>();
    for symbol in graph.symbols.values() {
        for pin in symbol.pins.values() {
            if pin.required_connection && pin.net_id.is_none() && !marked.contains(&pin.id) {
                findings.push(error(
                    "ERC.UNCONNECTED_REQUIRED",
                    vec![symbol.id.to_string(), pin.id.to_string()],
                    &pin.evidence_refs,
                    format!(
                        "required pin {}.{} is unconnected",
                        symbol.reference, pin.name
                    ),
                ));
            }
        }
    }
}

fn driver_conflicts(graph: &SchematicGraph, findings: &mut Vec<Finding>) {
    for net in graph.nets.values() {
        let drivers = net
            .pin_ids
            .iter()
            .filter_map(|pin_id| graph.pin(pin_id))
            .filter(|pin| pin.electrical_type == PinElectricalType::Output)
            .collect::<Vec<_>>();
        if drivers.len() > 1 {
            let mut refs = vec![net.id.to_string()];
            refs.extend(drivers.iter().map(|pin| pin.id.to_string()));
            findings.push(error(
                "ERC.DRIVER_CONFLICT",
                refs,
                &net.evidence_refs,
                format!(
                    "net {} has {} push-pull output drivers",
                    net.name,
                    drivers.len()
                ),
            ));
        }
    }
}

fn power_inputs_without_source(graph: &SchematicGraph, findings: &mut Vec<Finding>) {
    for net in graph.nets.values() {
        let pins = net
            .pin_ids
            .iter()
            .filter_map(|pin_id| graph.pin(pin_id))
            .collect::<Vec<_>>();
        let has_power_input = pins.iter().any(|pin| {
            pin.electrical_type == PinElectricalType::PowerInput
                || pin.power_role == PowerRole::Sink
        });
        let has_source = pins.iter().any(|pin| {
            pin.electrical_type == PinElectricalType::PowerOutput
                || pin.power_role == PowerRole::Source
        });
        if has_power_input && !has_source {
            findings.push(error(
                "ERC.POWER_NO_SOURCE",
                vec![net.id.to_string()],
                &net.evidence_refs,
                format!("power net {} has sinks but no legal source", net.name),
            ));
        }
    }
}

fn voltage_incompatibility(graph: &SchematicGraph, findings: &mut Vec<Finding>) {
    for net in graph.nets.values() {
        let Some(voltage) = net.voltage else {
            continue;
        };
        for pin_id in &net.pin_ids {
            let Some(pin) = graph.pin(pin_id) else {
                continue;
            };
            if let Some(allowed) = pin.allowed_voltage
                && !voltage.is_within(allowed)
            {
                findings.push(error(
                    "ERC.VOLTAGE_RANGE",
                    vec![net.id.to_string(), pin.id.to_string()],
                    &pin.evidence_refs,
                    format!(
                        "net {} range {:.3}-{:.3} V exceeds pin {} allowed {:.3}-{:.3} V",
                        net.name,
                        voltage.minimum_v,
                        voltage.maximum_v,
                        pin.name,
                        allowed.minimum_v,
                        allowed.maximum_v
                    ),
                ));
            }
        }
    }
}

fn footprint_bindings(graph: &SchematicGraph, context: &ErcContext, findings: &mut Vec<Finding>) {
    for symbol in graph.symbols.values() {
        let status = context.library_bindings.get(&symbol.id).cloned().unwrap_or(
            if symbol.footprint_asset_id.is_none() {
                LibraryBindingStatus::Missing
            } else {
                LibraryBindingStatus::Unverified
            },
        );
        if status != LibraryBindingStatus::VerifiedExact {
            findings.push(error(
                "ERC.FOOTPRINT_BINDING",
                vec![symbol.id.to_string()],
                &symbol.evidence_refs,
                format!(
                    "symbol {} footprint binding is not exact and verified: {status:?}",
                    symbol.reference
                ),
            ));
        }
    }
}

fn duplicate_i2c_addresses(graph: &SchematicGraph, findings: &mut Vec<Finding>) {
    let mut addresses: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for symbol in graph.symbols.values() {
        let (Some(bus), Some(address)) = (
            symbol.properties.get("i2c.bus"),
            symbol.properties.get("i2c.address"),
        ) else {
            continue;
        };
        addresses
            .entry((bus.to_ascii_lowercase(), normalize_address(address)))
            .or_default()
            .push(symbol.id.to_string());
    }
    for ((bus, address), objects) in addresses {
        if objects.len() > 1 {
            findings.push(error(
                "ERC.I2C_ADDRESS",
                objects,
                &[],
                format!("I2C bus {bus} has duplicate address {address}"),
            ));
        }
    }
}

fn decoupling_coverage(graph: &SchematicGraph, findings: &mut Vec<Finding>) {
    for symbol in graph.symbols.values() {
        if symbol
            .properties
            .get("decoupling.required")
            .map(String::as_str)
            != Some("true")
        {
            continue;
        }
        let covered = symbol
            .properties
            .get("decoupling.covered")
            .map(String::as_str)
            == Some("true");
        let value_present = symbol
            .properties
            .get("decoupling.value")
            .is_some_and(|value| !value.trim().is_empty());
        if !covered || !value_present {
            findings.push(error(
                "ERC.DECOUPLING",
                vec![symbol.id.to_string()],
                &symbol.evidence_refs,
                format!(
                    "{} lacks declared decoupling coverage or value",
                    symbol.reference
                ),
            ));
        }
    }
}

fn connector_contracts(graph: &SchematicGraph, findings: &mut Vec<Finding>) {
    for symbol in graph.symbols.values() {
        if symbol.properties.get("component.kind").map(String::as_str) != Some("connector") {
            continue;
        }
        let interface_ok = symbol
            .properties
            .get("connector.interface-mapped")
            .map(String::as_str)
            == Some("true");
        let direction_ok = symbol
            .properties
            .get("connector.direction-verified")
            .map(String::as_str)
            == Some("true");
        let protection_ok = symbol
            .properties
            .get("connector.protection-verified")
            .map(String::as_str)
            == Some("true");
        if !interface_ok || !direction_ok || !protection_ok {
            findings.push(error(
                "ERC.CONNECTOR_CONTRACT",
                vec![symbol.id.to_string()],
                &symbol.evidence_refs,
                format!(
                    "{} connector interface, direction or protection is incomplete",
                    symbol.reference
                ),
            ));
        }
    }
}

fn pin_mux_and_domains(graph: &SchematicGraph, findings: &mut Vec<Finding>) {
    for symbol in graph.symbols.values() {
        for pin in symbol.pins.values() {
            if pin.properties.get("mux.allowed").map(String::as_str) == Some("false") {
                findings.push(error(
                    "ERC.PINMUX_DOMAIN",
                    vec![symbol.id.to_string(), pin.id.to_string()],
                    &pin.evidence_refs,
                    format!("selected mux function is not allowed on pin {}", pin.name),
                ));
            }
            if let (Some(pin_domain), Some(net_id)) = (&pin.power_domain, &pin.net_id)
                && let Some(net) = graph.nets.get(net_id)
                && let Some(net_domain) = net
                    .name
                    .strip_prefix("DOMAIN:")
                    .map(str::to_owned)
                    .or_else(|| pin.properties.get("net.power-domain").cloned())
                && &net_domain != pin_domain
                && pin
                    .properties
                    .get("level-shifter.present")
                    .map(String::as_str)
                    != Some("true")
            {
                findings.push(error(
                    "ERC.PINMUX_DOMAIN",
                    vec![pin.id.to_string(), net.id.to_string()],
                    &pin.evidence_refs,
                    format!(
                        "pin domain {pin_domain} connects to {net_domain} without a declared level shifter"
                    ),
                ));
            }
        }
    }
}

fn dft_coverage(graph: &SchematicGraph, findings: &mut Vec<Finding>) {
    let covered = graph
        .test_points
        .values()
        .map(|test_point| &test_point.net_id)
        .collect::<BTreeSet<_>>();
    for net in graph.nets.values() {
        if net.requires_test_point && !covered.contains(&net.id) {
            findings.push(error(
                "ERC.DFT_COVERAGE",
                vec![net.id.to_string()],
                &net.evidence_refs,
                format!("net {} requires a test point but has none", net.name),
            ));
        }
    }
}

fn no_connect_conflicts(graph: &SchematicGraph, findings: &mut Vec<Finding>) {
    for marker in graph.no_connects.values() {
        let Some(pin) = graph.pin(&marker.pin_id) else {
            findings.push(error(
                "ERC.NO_CONNECT_CONFLICT",
                vec![marker.id.to_string(), marker.pin_id.to_string()],
                &marker.evidence_refs,
                "no-connect marker references a missing pin".to_owned(),
            ));
            continue;
        };
        if pin.net_id.is_some() {
            findings.push(error(
                "ERC.NO_CONNECT_CONFLICT",
                vec![marker.id.to_string(), pin.id.to_string()],
                &marker.evidence_refs,
                format!("pin {} is both connected and marked no-connect", pin.name),
            ));
        }
    }
}

fn assertion_margins(graph: &SchematicGraph, findings: &mut Vec<Finding>) {
    for symbol in graph.symbols.values() {
        let (Some(actual), Some(required)) = (
            symbol.properties.get("assertion.margin.actual"),
            symbol.properties.get("assertion.margin.required"),
        ) else {
            continue;
        };
        let actual = actual.parse::<f64>();
        let required = required.parse::<f64>();
        match (actual, required) {
            (Ok(actual), Ok(required)) if actual >= required => {}
            (Ok(actual), Ok(required)) => findings.push(error(
                "ERC.ASSERTION_MARGIN",
                vec![symbol.id.to_string()],
                &symbol.evidence_refs,
                format!(
                    "{} critical assertion margin {actual:.3} is below required {required:.3}",
                    symbol.reference
                ),
            )),
            _ => findings.push(error(
                "ERC.ASSERTION_MARGIN",
                vec![symbol.id.to_string()],
                &symbol.evidence_refs,
                "critical assertion margin is not a valid finite number".to_owned(),
            )),
        }
    }
}

fn normalize_address(value: &str) -> String {
    value
        .trim()
        .strip_prefix("0x")
        .unwrap_or(value.trim())
        .to_ascii_lowercase()
}

fn error(
    rule_id: &str,
    object_refs: Vec<String>,
    evidence_refs: &[String],
    message: impl Into<String>,
) -> Finding {
    Finding::new(
        rule_id,
        VERSION,
        Severity::Error,
        object_refs,
        evidence_refs.to_vec(),
        message,
        vec![SuggestedAction {
            kind: "repair".to_owned(),
            description: "Repair the referenced design objects and re-run native ERC.".to_owned(),
        }],
        FindingCoverage {
            status: "implemented".to_owned(),
            checked_aspects: vec!["structured logical graph facts".to_owned()],
            gaps: rule_manifest()
                .into_iter()
                .find(|rule| rule.rule_id == rule_id)
                .map_or_else(Vec::new, |rule| rule.gaps),
        },
    )
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use jingwei_jir::{EditControl, EntityId};
    use jingwei_schematic::{
        Net, NetClass, NoConnect, PinInstance, PointMm, PowerRole, SchematicGraph, Sheet,
        SymbolInstance, TestPoint, VoltageRange,
    };

    use super::*;
    use crate::{ErcContext, LibraryBindingStatus, run_native_erc};

    fn id(value: &str) -> EntityId {
        EntityId::new(value).expect("valid id")
    }

    fn valid_graph() -> (SchematicGraph, ErcContext) {
        let sheet_id = id("sheet:root");
        let source_id = id("symbol:source");
        let load_id = id("symbol:load");
        let source_pin_id = id("pin:source:1");
        let load_pin_id = id("pin:load:1");
        let net_id = id("net:v3v3");
        let mut graph = SchematicGraph {
            id: id("schematic:test"),
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
            ..SchematicGraph::default()
        };
        graph.symbols.insert(
            source_id.clone(),
            symbol(
                source_id.clone(),
                sheet_id.clone(),
                "U1",
                source_pin_id.clone(),
                PinElectricalType::PowerOutput,
                PowerRole::Source,
                net_id.clone(),
            ),
        );
        let mut load = symbol(
            load_id.clone(),
            sheet_id.clone(),
            "U2",
            load_pin_id.clone(),
            PinElectricalType::PowerInput,
            PowerRole::Sink,
            net_id.clone(),
        );
        load.properties.extend([
            ("decoupling.required".to_owned(), "true".to_owned()),
            ("decoupling.covered".to_owned(), "true".to_owned()),
            ("decoupling.value".to_owned(), "100 nF".to_owned()),
            ("assertion.margin.actual".to_owned(), "0.25".to_owned()),
            ("assertion.margin.required".to_owned(), "0.20".to_owned()),
        ]);
        graph.symbols.insert(load_id.clone(), load);
        graph.nets.insert(
            net_id.clone(),
            Net {
                id: net_id.clone(),
                sheet_id,
                name: "V3V3".to_owned(),
                class: NetClass::Power,
                pin_ids: BTreeSet::from([source_pin_id, load_pin_id]),
                voltage: Some(VoltageRange {
                    minimum_v: 3.2,
                    maximum_v: 3.4,
                }),
                current_budget_a: Some(0.5),
                requires_test_point: true,
                evidence_refs: Vec::new(),
            },
        );
        graph.test_points.insert(
            id("test-point:tp1"),
            TestPoint {
                id: id("test-point:tp1"),
                net_id,
                reference: "TP1".to_owned(),
                position: PointMm { x: 0.0, y: 0.0 },
                requirement_refs: vec!["requirement:dft".to_owned()],
            },
        );
        let context = ErcContext {
            library_bindings: BTreeMap::from([
                (source_id, LibraryBindingStatus::VerifiedExact),
                (load_id, LibraryBindingStatus::VerifiedExact),
            ]),
            project_risk_profile: "low-voltage-general".to_owned(),
        };
        (graph, context)
    }

    fn symbol(
        symbol_id: EntityId,
        sheet_id: EntityId,
        reference: &str,
        pin_id: EntityId,
        electrical_type: PinElectricalType,
        power_role: PowerRole,
        net_id: EntityId,
    ) -> SymbolInstance {
        SymbolInstance {
            id: symbol_id.clone(),
            sheet_id,
            module_id: None,
            reference: reference.to_owned(),
            value: "fixture".to_owned(),
            exact_mpn: format!("JW-{reference}"),
            part_decision_id: format!("part-decision:{reference}"),
            symbol_asset_id: format!("symbol-asset:{reference}"),
            footprint_asset_id: Some(format!("footprint-asset:{reference}")),
            position: PointMm { x: 0.0, y: 0.0 },
            rotation_deg: 0.0,
            mirrored: false,
            pins: BTreeMap::from([(
                pin_id.clone(),
                PinInstance {
                    id: pin_id,
                    symbol_id,
                    number: "1".to_owned(),
                    name: "VDD".to_owned(),
                    electrical_type,
                    power_role,
                    required_connection: true,
                    net_id: Some(net_id),
                    allowed_voltage: Some(VoltageRange {
                        minimum_v: 3.0,
                        maximum_v: 3.6,
                    }),
                    power_domain: Some("V3V3".to_owned()),
                    interface_id: None,
                    properties: BTreeMap::new(),
                    evidence_refs: Vec::new(),
                },
            )]),
            properties: BTreeMap::new(),
            evidence_refs: Vec::new(),
            edit_control: EditControl::default(),
        }
    }

    #[test]
    fn clean_fixture_has_zero_native_errors_and_reports_gaps() {
        let (graph, context) = valid_graph();
        let run = run_native_erc(&graph, &context);
        assert_eq!(run.error_count, 0, "{:?}", run.findings);
        assert!(run.passed);
        assert!(run.coverage.iter().filter(|rule| rule.implemented).count() >= 10);
        assert!(run.coverage.iter().any(|rule| !rule.implemented));
    }

    #[test]
    fn negative_mutations_trigger_all_twelve_rules() {
        let (mut graph, mut context) = valid_graph();
        let load_id = id("symbol:load");
        let source_id = id("symbol:source");
        let load_pin_id = id("pin:load:1");
        let source_pin_id = id("pin:source:1");
        let net_id = id("net:v3v3");

        graph
            .symbols
            .get_mut(&load_id)
            .expect("load")
            .pins
            .get_mut(&load_pin_id)
            .expect("pin")
            .required_connection = true;
        graph
            .symbols
            .get_mut(&load_id)
            .expect("load")
            .pins
            .get_mut(&load_pin_id)
            .expect("pin")
            .net_id = None;
        graph
            .nets
            .get_mut(&net_id)
            .expect("net")
            .pin_ids
            .remove(&load_pin_id);
        graph
            .symbols
            .get_mut(&source_id)
            .expect("source")
            .pins
            .get_mut(&source_pin_id)
            .expect("pin")
            .electrical_type = PinElectricalType::Output;
        graph
            .symbols
            .get_mut(&source_id)
            .expect("source")
            .pins
            .get_mut(&source_pin_id)
            .expect("pin")
            .power_role = PowerRole::None;
        let second_output = id("pin:load:out");
        graph.symbols.get_mut(&load_id).expect("load").pins.insert(
            second_output.clone(),
            PinInstance {
                id: second_output.clone(),
                symbol_id: load_id.clone(),
                number: "2".to_owned(),
                name: "OUT".to_owned(),
                electrical_type: PinElectricalType::Output,
                power_role: PowerRole::Sink,
                required_connection: true,
                net_id: Some(net_id.clone()),
                allowed_voltage: Some(VoltageRange {
                    minimum_v: 0.0,
                    maximum_v: 1.0,
                }),
                power_domain: None,
                interface_id: None,
                properties: BTreeMap::from([("mux.allowed".to_owned(), "false".to_owned())]),
                evidence_refs: Vec::new(),
            },
        );
        graph
            .nets
            .get_mut(&net_id)
            .expect("net")
            .pin_ids
            .insert(second_output.clone());
        context
            .library_bindings
            .insert(load_id.clone(), LibraryBindingStatus::PackageMismatch);
        graph
            .symbols
            .get_mut(&source_id)
            .expect("source")
            .properties
            .extend([
                ("i2c.bus".to_owned(), "I2C1".to_owned()),
                ("i2c.address".to_owned(), "0x44".to_owned()),
            ]);
        graph
            .symbols
            .get_mut(&load_id)
            .expect("load")
            .properties
            .extend([
                ("i2c.bus".to_owned(), "I2C1".to_owned()),
                ("i2c.address".to_owned(), "44".to_owned()),
                ("decoupling.covered".to_owned(), "false".to_owned()),
                ("component.kind".to_owned(), "connector".to_owned()),
                ("connector.interface-mapped".to_owned(), "false".to_owned()),
                ("assertion.margin.actual".to_owned(), "0.1".to_owned()),
            ]);
        graph.test_points.clear();
        graph.no_connects.insert(
            id("no-connect:1"),
            NoConnect {
                id: id("no-connect:1"),
                pin_id: second_output,
                evidence_refs: Vec::new(),
            },
        );
        let run = run_native_erc(&graph, &context);
        let rule_ids = run
            .findings
            .iter()
            .map(|finding| finding.rule_id.as_str())
            .collect::<BTreeSet<_>>();
        for expected in [
            "ERC.UNCONNECTED_REQUIRED",
            "ERC.DRIVER_CONFLICT",
            "ERC.POWER_NO_SOURCE",
            "ERC.VOLTAGE_RANGE",
            "ERC.FOOTPRINT_BINDING",
            "ERC.I2C_ADDRESS",
            "ERC.DECOUPLING",
            "ERC.CONNECTOR_CONTRACT",
            "ERC.PINMUX_DOMAIN",
            "ERC.DFT_COVERAGE",
            "ERC.NO_CONNECT_CONFLICT",
            "ERC.ASSERTION_MARGIN",
        ] {
            assert!(
                rule_ids.contains(expected),
                "missing {expected}: {rule_ids:?}"
            );
        }
    }
}
