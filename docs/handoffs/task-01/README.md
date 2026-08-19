# Task 01 handoff: Agent Runtime control plane

Status: control-plane implementation complete; commercial provider and Rust daemon integration gates remain explicit in
`open-gaps.md`. Cross-runtime Runtime → JDP → Rust daemon tracing is **blocked and not accepted**.

## Delivered

- Seven brand-neutral packages under `packages/` with one public entry point per package.
- Bun/TypeScript workspace, reproducible pnpm lockfile, strict typecheck, Biome lint, tsup builds.
- Durable SQLite session/input/run/model-attempt/event/tool/approval/artifact state.
- Idempotent input admission, `steer`/`queue`, session coordinator, provider attempt isolation, cancellation, and recovery.
- Canonical registry/settlement pipeline for builtin, generated JDP, MCP, and plugin tools.
- Layered deny/ask/allow policy, scoped durable approvals, root-only specialist tools, and resource leases.
- MCP server isolation, explicit allowlists, credential references, worst-case effects, stale identities, timeouts, limits, and audit.
- Runtime-side OTel adapters, OTLP export, parent-context handling, and safe attribute allowlist. This is not Rust daemon
  cross-runtime trace evidence.
- Official OpenAI Responses SDK adapter with streaming/tool continuation and a credential-gated live contract.
- Rust codegen execution, reproducibility check, generated catalog consumption, and production JDP adapter registration.
- Authenticated MCP Streamable HTTP integration against an official SDK server in a separate process.
- OTLP/HTTP exporter lifecycle and real collector wire-payload verification.
- Explicit `MockJdpClient` retained for deterministic fault/recovery contracts only.
- Fault-injection fixtures, negative security tests, and this handoff evidence package.

## Navigation

- `final-delivery.md`: final delivery statement, acceptance summary, integration gates, and recommended handoff order.
- `implementation-map.md`: package responsibilities and extraction mapping.
- `test-report.md`: commands, results, and real-versus-mock classification.
- `recovery-and-security.md`: kill points and negative security evidence.
- `source-provenance.md`: reference commit and code provenance.
- `open-gaps.md`: unclosed live-credential, daemon, recovery, and compatibility work.
- `otel-trace-example.json`: sanitized Runtime-only example; explicitly not evidence of a Rust daemon child span.
