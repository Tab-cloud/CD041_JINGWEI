# @jingwei/agent-tools

The canonical registry and only settlement pipeline for builtin, generated JDP, MCP, and plugin tools. Every call is
durably recorded before side effects, checked against its advertised identity, authorized independently of visibility,
optionally approved, bounded/redacted, and settled to a typed terminal event. Unknown side effects are never retried.

`registerRustGeneratedJdpCatalog()` consumes the Rust `jingwei-codegen` tool catalog, enforces protocol/schema compatibility,
maps Rust permission/effect metadata, and registers every generated operation through the same production JDP adapter.
