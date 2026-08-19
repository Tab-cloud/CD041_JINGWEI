//! Stable JDP envelopes, operations, events, display/edit contracts, and client abstraction.

mod catalog;
mod codegen;
mod envelope;
mod operations;
mod view;

pub use catalog::{Effect, Permission, ToolDescriptor, tool_catalog};
pub use codegen::{CodegenError, generate_bindings};
pub use envelope::{
    ClientError, ErrorDetail, FindingSummary, JdpClient, JdpFailure, JdpRequest, JdpResponse,
    JdpSuccess, SuggestedAction, SuggestedActionKind,
};
pub use operations::*;
pub use view::*;

/// Current JDP wire protocol.
pub const JDP_PROTOCOL_VERSION: &str = "0.1";
/// Current durable-event schema.
pub const EVENT_SCHEMA_VERSION: &str = "0.1";
/// Maximum accepted ordinary JDP frame size.
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
