//! Canonical JDL text formatter.

use std::fmt::Write;

use crate::{AstValue, Item, JdlDocument};

/// Formats a parsed document deterministically.
pub fn format(document: &JdlDocument) -> String {
    let mut output = String::new();
    writeln!(output, "project {} {{", document.project_name).expect("string writes cannot fail");
    format_items(&mut output, &document.items, 1);
    output.push_str("}\n");
    output
}

fn format_items(output: &mut String, items: &[Item], depth: usize) {
    let indent = "  ".repeat(depth);
    for item in items {
        match item {
            Item::Module { name, items, .. } | Item::Block { name, items, .. } => {
                let keyword = if matches!(item, Item::Module { .. }) {
                    "module"
                } else {
                    "block"
                };
                writeln!(output, "{indent}{keyword} {name} {{").expect("string writes cannot fail");
                format_items(output, items, depth + 1);
                writeln!(output, "{indent}}}").expect("string writes cannot fail");
            }
            Item::Interface { name, .. } => writeln!(output, "{indent}interface {name};").unwrap(),
            Item::Component {
                name,
                component_type,
                ..
            } => {
                if let Some(component_type) = component_type {
                    writeln!(
                        output,
                        "{indent}component {name}: \"{}\";",
                        escape(component_type)
                    )
                    .unwrap();
                } else {
                    writeln!(output, "{indent}component {name};").unwrap();
                }
            }
            Item::Net { name, .. } => writeln!(output, "{indent}net {name};").unwrap(),
            Item::Property {
                name,
                property_type,
                value,
                ..
            } => {
                writeln!(
                    output,
                    "{indent}property {name}: {property_type} = {};",
                    format_value(value)
                )
                .unwrap();
            }
            Item::Connect { from, to, .. } => {
                writeln!(output, "{indent}connect {from} -> {to};").unwrap();
            }
            Item::Constraint {
                name, expression, ..
            } => writeln!(output, "{indent}constraint {name} = {expression};").unwrap(),
            Item::Assertion { expression, .. } => {
                writeln!(output, "{indent}assert {expression};").unwrap();
            }
        }
    }
}

fn format_value(value: &AstValue) -> String {
    match value {
        AstValue::String(value) => format!("\"{}\"", escape(value)),
        AstValue::Identifier(value) | AstValue::Quantity(value) => value.clone(),
        AstValue::Boolean(value) => value.to_string(),
    }
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
