//! Single-writer JDP daemon, in-memory client, and Windows named-pipe transport.

mod daemon;
mod transport;

pub use daemon::{Daemon, DaemonError, DaemonSpan, HealthStatus, ReadinessStatus};
pub use transport::InMemoryClient;

/// Composes the production Phase 0 logical and physical domain registry.
pub fn production_registry()
-> Result<jingwei_kernel::ExtensionRegistry, jingwei_kernel::RegistryError> {
    let builder = jingwei_kicad_schematic::register_logical_module(
        jingwei_kernel::ExtensionRegistryBuilder::new(),
    )?;
    let builder = jingwei_manufacturing::register_physical_module(builder)?;
    Ok(builder.build())
}

#[cfg(windows)]
pub use transport::{NamedPipeClient, serve_named_pipe_once};
