# Runtime test fixtures

This directory contains versioned, non-production fixtures owned by task 01. The JDP fixture mirrors contract-pack v0 only
until the Rust-generated TypeScript binding is merged. It is not a second normative protocol definition.

- `jdp/contract-pack-v0.mock.json`: mock request/success/reconciliation envelopes.
- `fault-injection-cases.json`: the eight required restart boundaries and expected safe outcomes.
- `provider/partial-stream.json`: scripted uncommitted provider output followed by a transport failure.
