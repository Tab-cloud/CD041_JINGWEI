//! Deterministic JDL AST to extensible JIR graph compiler.

use std::collections::BTreeMap;

use jingwei_jir::{
    CanonicalValue, EdgeId, EditControl, EntityId, EntityKind, JirEdge, JirEntity, JirGraph,
    Provenance,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{AstValue, Diagnostic, DiagnosticSeverity, Item, JdlDocument, SourceSpan};

/// Successful compiler output.
#[derive(Clone, Debug)]
pub struct CompileOutput {
    pub graph: JirGraph,
    pub diagnostics: Vec<Diagnostic>,
}

/// Deterministic semantic compilation failure.
#[derive(Clone, Debug, thiserror::Error)]
#[error("JDL compilation failed with {diagnostics_len} diagnostic(s)")]
pub struct CompileError {
    pub diagnostics: Vec<Diagnostic>,
    diagnostics_len: usize,
}

/// Compiles a parsed project into a graph. No file, network, process, or code execution occurs.
pub fn compile(document: &JdlDocument) -> Result<CompileOutput, CompileError> {
    let mut compiler = Compiler {
        document,
        graph: JirGraph::default(),
        names: BTreeMap::new(),
        connects: Vec::new(),
        diagnostics: Vec::new(),
    };
    let project_path = document.project_name.clone();
    let project_id = stable_entity_id(&format!("project:{project_path}"));
    compiler.add_entity(
        project_id.clone(),
        "core.project",
        json!({"name": document.project_name, "astVersion": document.ast_version}),
        document.span,
    );
    compiler
        .names
        .insert(project_path.clone(), project_id.clone());
    compiler
        .names
        .insert(document.project_name.clone(), project_id.clone());
    compiler.walk_items(&document.items, &project_path, &project_id);
    compiler.finish_connects();
    if compiler
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    {
        return Err(CompileError {
            diagnostics_len: compiler.diagnostics.len(),
            diagnostics: compiler.diagnostics,
        });
    }
    Ok(CompileOutput {
        graph: compiler.graph,
        diagnostics: compiler.diagnostics,
    })
}

struct PendingConnect {
    scope: String,
    from: String,
    to: String,
    span: SourceSpan,
}

struct Compiler<'a> {
    document: &'a JdlDocument,
    graph: JirGraph,
    names: BTreeMap<String, EntityId>,
    connects: Vec<PendingConnect>,
    diagnostics: Vec<Diagnostic>,
}

impl Compiler<'_> {
    fn walk_items(&mut self, items: &[Item], scope: &str, parent: &EntityId) {
        for (ordinal, item) in items.iter().enumerate() {
            match item {
                Item::Module { name, items, span } | Item::Block { name, items, span } => {
                    let path = format!("{scope}.{name}");
                    let id = stable_entity_id(&format!("scope:{path}"));
                    let kind = if matches!(item, Item::Module { .. }) {
                        "core.module"
                    } else {
                        "core.block"
                    };
                    self.add_named_entity(
                        &path,
                        name,
                        id.clone(),
                        kind,
                        json!({"name": name}),
                        *span,
                    );
                    self.add_contains(parent, &id, ordinal);
                    self.walk_items(items, &path, &id);
                }
                Item::Interface { name, span } => {
                    let path = format!("{scope}.{name}");
                    let id = stable_entity_id(&format!("interface:{path}"));
                    self.add_named_entity(
                        &path,
                        name,
                        id.clone(),
                        "core.interface",
                        json!({"name": name}),
                        *span,
                    );
                    self.add_contains(parent, &id, ordinal);
                }
                Item::Component {
                    name,
                    component_type,
                    span,
                } => {
                    let path = format!("{scope}.{name}");
                    let id = stable_entity_id(&format!("component:{path}"));
                    self.add_named_entity(
                        &path,
                        name,
                        id.clone(),
                        "core.component",
                        json!({"name": name, "componentType": component_type}),
                        *span,
                    );
                    self.add_contains(parent, &id, ordinal);
                }
                Item::Net { name, span } => {
                    let path = format!("{scope}.{name}");
                    let id = stable_entity_id(&format!("net:{path}"));
                    self.add_named_entity(
                        &path,
                        name,
                        id.clone(),
                        "core.net",
                        json!({"name": name}),
                        *span,
                    );
                    self.add_contains(parent, &id, ordinal);
                }
                Item::Property {
                    name,
                    property_type,
                    value,
                    span,
                } => {
                    let path = format!("{scope}.{name}");
                    let id = stable_entity_id(&format!("property:{path}"));
                    let Some(value) = self.compile_value(value, property_type, *span) else {
                        continue;
                    };
                    self.add_named_entity(
                        &path,
                        name,
                        id.clone(),
                        "core.property",
                        json!({"name": name, "propertyType": property_type, "value": value}),
                        *span,
                    );
                    self.add_contains(parent, &id, ordinal);
                }
                Item::Constraint {
                    name,
                    expression,
                    span,
                } => {
                    let path = format!("{scope}.{name}");
                    let id = stable_entity_id(&format!("constraint:{path}"));
                    self.add_named_entity(
                        &path,
                        name,
                        id.clone(),
                        "core.constraint",
                        json!({"name": name, "expression": expression, "strength": "hard"}),
                        *span,
                    );
                    self.add_contains(parent, &id, ordinal);
                }
                Item::Assertion { expression, span } => {
                    let path = format!("{scope}.assertion-{ordinal}");
                    let id = stable_entity_id(&format!("assertion:{path}:{expression}"));
                    self.add_entity(
                        id.clone(),
                        "core.assertion",
                        json!({"expression": expression}),
                        *span,
                    );
                    self.add_contains(parent, &id, ordinal);
                }
                Item::Connect { from, to, span } => self.connects.push(PendingConnect {
                    scope: scope.into(),
                    from: from.clone(),
                    to: to.clone(),
                    span: *span,
                }),
            }
        }
    }

    fn compile_value(
        &mut self,
        value: &AstValue,
        property_type: &str,
        span: SourceSpan,
    ) -> Option<Value> {
        match value {
            AstValue::String(value) | AstValue::Identifier(value) => {
                Some(Value::String(value.clone()))
            }
            AstValue::Boolean(value) => Some(Value::Bool(*value)),
            AstValue::Quantity(value) => match parse_quantity(value, property_type) {
                Ok(quantity) => Some(quantity),
                Err(message) => {
                    self.diagnostic("JDL_INVALID_QUANTITY", message, span);
                    None
                }
            },
        }
    }

    fn add_named_entity(
        &mut self,
        path: &str,
        short_name: &str,
        id: EntityId,
        kind: &str,
        properties: Value,
        span: SourceSpan,
    ) {
        if self.names.contains_key(path) {
            self.diagnostic(
                "JDL_DUPLICATE_NAME",
                format!("duplicate declaration `{path}`"),
                span,
            );
            return;
        }
        self.add_entity(id.clone(), kind, properties, span);
        self.names.insert(path.into(), id.clone());
        self.names.entry(short_name.into()).or_insert(id);
    }

    fn add_entity(&mut self, id: EntityId, kind: &str, properties: Value, span: SourceSpan) {
        let entity = JirEntity {
            id,
            kind: EntityKind::new(kind).expect("built-in kinds are valid"),
            schema_version: jingwei_jir::JIR_SCHEMA_VERSION.into(),
            properties: CanonicalValue::new(properties).expect("compiler values are canonical"),
            provenance: Provenance {
                source_kind: "jdl".into(),
                evidence_refs: Vec::new(),
                generator: Some("jingwei-jdl".into()),
                generator_version: Some(env!("CARGO_PKG_VERSION").into()),
            },
            edit_control: EditControl::default(),
        };
        if let Err(error) = self.graph.insert_entity(entity) {
            self.diagnostic("JDL_GRAPH_ENTITY", error.to_string(), span);
        }
    }

    fn add_contains(&mut self, parent: &EntityId, child: &EntityId, ordinal: usize) {
        let seed = format!("contains:{parent}:{child}");
        let edge = JirEdge {
            id: stable_edge_id(&seed),
            kind: "core.contains".into(),
            from: parent.clone(),
            to: child.clone(),
            properties: CanonicalValue::new(json!({"ordinal": ordinal})).unwrap(),
        };
        if let Err(error) = self.graph.insert_edge(edge) {
            self.diagnostic("JDL_GRAPH_EDGE", error.to_string(), SourceSpan::default());
        }
    }

    fn finish_connects(&mut self) {
        for pending in std::mem::take(&mut self.connects) {
            let from = self.resolve(&pending.scope, &pending.from);
            let to = self.resolve(&pending.scope, &pending.to);
            match (from, to) {
                (Some(from), Some(to)) => {
                    let edge = JirEdge {
                        id: stable_edge_id(&format!(
                            "connect:{}:{}:{}",
                            pending.scope, pending.from, pending.to
                        )),
                        kind: "core.connects".into(),
                        from,
                        to,
                        properties: CanonicalValue::new(json!({
                            "fromEndpoint": pending.from,
                            "toEndpoint": pending.to,
                        }))
                        .unwrap(),
                    };
                    if let Err(error) = self.graph.insert_edge(edge) {
                        self.diagnostic("JDL_CONNECT_FAILED", error.to_string(), pending.span);
                    }
                }
                _ => self.diagnostic(
                    "JDL_UNRESOLVED_CONNECT",
                    format!(
                        "cannot resolve connection `{} -> {}`",
                        pending.from, pending.to
                    ),
                    pending.span,
                ),
            }
        }
    }

    fn resolve(&self, scope: &str, path: &str) -> Option<EntityId> {
        let first = path.split('.').next().unwrap_or(path);
        let mut candidate_scope = scope;
        loop {
            let candidate = format!("{candidate_scope}.{first}");
            if let Some(id) = self.names.get(&candidate) {
                return Some(id.clone());
            }
            if let Some((parent, _)) = candidate_scope.rsplit_once('.') {
                candidate_scope = parent;
            } else {
                break;
            }
        }
        self.names.get(first).cloned()
    }

    fn diagnostic(&mut self, code: &str, message: impl Into<String>, span: SourceSpan) {
        self.diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Error,
            code: code.into(),
            message: message.into(),
            source_name: self.document.source_name.clone(),
            span,
        });
    }
}

fn parse_quantity(raw: &str, property_type: &str) -> Result<Value, String> {
    let split = raw
        .find(|character: char| !character.is_ascii_digit() && character != '.' && character != '-')
        .unwrap_or(raw.len());
    let (number, unit) = raw.split_at(split);
    let value: f64 = number
        .parse()
        .map_err(|_| format!("invalid numeric literal `{raw}`"))?;
    if !value.is_finite() || unit.is_empty() {
        return Err(format!(
            "physical property `{property_type}` requires a finite value and unit"
        ));
    }
    let dimension = match unit {
        "m" | "mm" | "um" => "length",
        "V" | "mV" => "voltage",
        "A" | "mA" => "current",
        "degC" => "temperature",
        "deg" | "rad" => "angle",
        "s" | "ms" => "time",
        "%" => "dimensionless",
        _ => return Err(format!("unsupported unit `{unit}`")),
    };
    let expected = property_type.to_ascii_lowercase();
    if matches!(
        expected.as_str(),
        "length" | "voltage" | "current" | "temperature" | "angle" | "time"
    ) && expected != dimension
    {
        return Err(format!(
            "property type `{property_type}` is incompatible with unit `{unit}`"
        ));
    }
    Ok(json!({"value": value, "unit": unit, "dimension": dimension}))
}

fn stable_entity_id(seed: &str) -> EntityId {
    EntityId::new(format!("entity-{}", digest_prefix(seed))).expect("generated ID is valid")
}

fn stable_edge_id(seed: &str) -> EdgeId {
    EdgeId::new(format!("edge-{}", digest_prefix(seed))).expect("generated ID is valid")
}

fn digest_prefix(seed: &str) -> String {
    let digest = hex::encode(Sha256::digest(seed.as_bytes()));
    digest[..24].to_owned()
}

#[cfg(test)]
mod tests {
    use crate::{ParseLimits, format, parse};

    use super::*;

    const SOURCE: &str = r#"
project sensor {
  interface input;
  module core {
    component U1: "controller";
    net VCC;
    property supply: voltage = 3.3V;
    connect U1.VCC -> VCC;
    assert U1 connected VCC;
  }
}
"#;

    #[test]
    fn parse_format_parse_round_trip_and_stable_graph() {
        let first = parse("fixture.jdl", SOURCE, ParseLimits::default()).unwrap();
        let formatted = format(&first);
        let second = parse("fixture.jdl", &formatted, ParseLimits::default()).unwrap();
        assert_eq!(first.project_name, second.project_name);
        assert_eq!(format(&second), formatted);
        let first_graph = compile(&first).unwrap().graph;
        let second_graph = compile(&second).unwrap().graph;
        assert_eq!(
            first_graph.content_hash().unwrap(),
            second_graph.content_hash().unwrap()
        );
    }

    #[test]
    fn rejects_wrong_quantity_dimension() {
        let document = parse(
            "bad.jdl",
            "project bad { property voltage: voltage = 3mm; }",
            ParseLimits::default(),
        )
        .unwrap();
        assert!(compile(&document).is_err());
    }
}
