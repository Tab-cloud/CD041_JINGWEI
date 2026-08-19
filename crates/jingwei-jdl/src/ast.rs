//! Versioned JDL syntax tree and diagnostics.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Half-open UTF-8 byte range in one source file.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

/// Parsed literal. JDL has no code-execution value.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum AstValue {
    String(String),
    Identifier(String),
    Quantity(String),
    Boolean(bool),
}

/// Parsed project document.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JdlDocument {
    pub ast_version: String,
    pub source_name: String,
    pub project_name: String,
    pub items: Vec<Item>,
    pub span: SourceSpan,
}

/// Phase 0 declarations.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Item {
    Module {
        name: String,
        items: Vec<Item>,
        span: SourceSpan,
    },
    Block {
        name: String,
        items: Vec<Item>,
        span: SourceSpan,
    },
    Interface {
        name: String,
        span: SourceSpan,
    },
    Component {
        name: String,
        component_type: Option<String>,
        span: SourceSpan,
    },
    Net {
        name: String,
        span: SourceSpan,
    },
    Property {
        name: String,
        property_type: String,
        value: AstValue,
        span: SourceSpan,
    },
    Connect {
        from: String,
        to: String,
        span: SourceSpan,
    },
    Constraint {
        name: String,
        expression: String,
        span: SourceSpan,
    },
    Assertion {
        expression: String,
        span: SourceSpan,
    },
}

impl Item {
    /// Returns the source span of this declaration.
    pub const fn span(&self) -> SourceSpan {
        match self {
            Self::Module { span, .. }
            | Self::Block { span, .. }
            | Self::Interface { span, .. }
            | Self::Component { span, .. }
            | Self::Net { span, .. }
            | Self::Property { span, .. }
            | Self::Connect { span, .. }
            | Self::Constraint { span, .. }
            | Self::Assertion { span, .. } => *span,
        }
    }
}

/// Diagnostic severity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

/// Bounded source diagnostic.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub source_name: String,
    pub span: SourceSpan,
}
