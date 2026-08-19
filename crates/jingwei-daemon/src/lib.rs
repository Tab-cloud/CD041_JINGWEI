//! Single-writer JDP daemon, in-memory client, and Windows named-pipe transport.

mod daemon;
mod transport;

pub use daemon::{Daemon, DaemonError, HealthStatus, ReadinessStatus};
pub use transport::InMemoryClient;

#[cfg(windows)]
pub use transport::{NamedPipeClient, serve_named_pipe_once};
