# @jingwei/agent-observability

OpenTelemetry span adapters and a strict attribute allowlist. Prompt bodies, design payloads, secrets, credentials, and
large tool results are deliberately excluded. Durable audit records remain separate from telemetry.

`createOtlpTracing()` creates an isolated SDK provider with a batched OTLP/HTTP JSON exporter and explicit flush/shutdown
lifecycle. The integration test exports a propagated `jdp.request` trace to a real loopback HTTP collector and inspects the
wire payload for trace continuity and secret/private-attribute exclusion.
