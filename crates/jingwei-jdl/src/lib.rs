//! Minimal, deterministic and non-executable JDL Phase 0 language.

mod ast;
mod compiler;
mod formatter;
mod parser;

pub use ast::{AstValue, Diagnostic, DiagnosticSeverity, Item, JdlDocument, SourceSpan};
pub use compiler::{CompileError, CompileOutput, compile};
pub use formatter::format;
pub use parser::{ParseError, ParseLimits, parse};

/// Version of the Phase 0 AST contract.
pub const JDL_AST_VERSION: &str = "0.1";
