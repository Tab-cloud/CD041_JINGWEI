# Task 01 interface proposal: JDP trace-context propagation

Status: blocked pending formal task 02 `jingwei-daemon`, `bindings/typescript/**`, and Rust trace-context receiver.

## Problem

The Rust-generated JDP `0.1` request contains no trace context. The runtime can create a `jdp.request` span and export it over
OTLP, but the daemon cannot create a child span in the same trace from the generated envelope. The generated TypeScript
`dryRun` property is also required at type level while the Rust schema supplies a default and does not require it.

## Proposed compatible change

Add an optional transport-neutral field to the Rust source type and generated binding:

```text
traceContext?: { traceId: string; spanId: string; traceFlags?: number }
```

The daemon must validate W3C-sized hexadecimal IDs, treat the context as remote, create its request span as a child, and never
accept baggage or arbitrary telemetry attributes in this field. Named-pipe implementations may carry the same values as
authenticated frame metadata, but the generated client must expose one stable API for both choices.

Also make generated `dryRun` optional to match the JSON Schema default, or remove the schema default and require it in both
Rust and TypeScript. Task 01 already serializes an explicit boolean, so either normalized wire choice is compatible.

## Acceptance

- Regenerated TypeScript and JSON Schema agree on optionality.
- A runtime `jdp.request` and daemon handler span arrive at one OTLP collector with the same trace ID and correct parent ID.
- Invalid trace IDs are ignored or rejected with a typed transport error; they never crash the daemon.
- Prompt, JIR, credential, and tool-output content remain absent from span attributes.

Runtime-only OTLP export, supplied-parent tests, recording clients, generated temporary files, and example JSON do not satisfy
this acceptance. The gate closes only with the real task 02 daemon and product binding path running in the test.
