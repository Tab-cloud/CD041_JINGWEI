# Open integration gates

These items are intentionally not reported as complete:

1. Task 02 has not formally delivered a production `jingwei-daemon`, checked-in `bindings/typescript/**`, or a Rust
   trace-context receiver/validator that creates the daemon handler child span. These are three separate required artifacts.
   Task 01 now runs the real Rust generator, proves reproducibility, imports its generated constants, and consumes its tool
   catalog, but does not write task 02-owned paths or substitute a test daemon for the product daemon.
2. `OpenAIResponsesProvider` is implemented and tested through the official SDK over real loopback SSE. A commercial endpoint
   call was not performed because neither `OPENAI_API_KEY` nor `JINGWEI_OPENAI_MODEL` was available. The live contract is
   explicit and will run when both are supplied; no credential is read from files, fixtures, databases, or logs.
3. The current Rust-generated JDP envelope has no trace-context field. TypeScript OTLP export and JDP parent propagation are
   verified independently, but a Rust daemon child span cannot be proven until task 02 implements the optional propagation
   field or equivalent IPC metadata described in `docs/interface-change-proposals/task-01-jdp-trace-context.md`.
   Consequently, cross-runtime tracing is `BLOCKED / NOT ACCEPTED`; Runtime-only spans, loopback collectors, generated
   temporary bindings, recording clients, and examples cannot close this gate.
4. A recorded pre-handler execution is classified as safe to resume, but automatic resumption requires the host to inject the
   `resumeRecorded` callback with the original authenticated execution context.
5. Pending approvals survive restart, but the original in-memory coroutine is not resurrected. The recovered run is requeued
   and reconstructs its continuation from durable input/tool/approval facts.
6. SQLite repositories share one file but append their object and durable event in ordered, separate transactions. A crash
   between them leaves a recoverable object without its presentation event; an atomic outbox migration is recommended before
   multi-process production use.
7. Stdio MCP process isolation remains a platform-specific transport-factory responsibility. Streamable HTTP is now covered by
   a real authenticated separate-process server, including initialize, session/protocol headers, discovery, call, and rejection.
8. Windows Bun keeps some closed SQLite test files locked until process exit; tests tolerate only `EBUSY` for OS temp cleanup.
   No runtime database handle remains intentionally open.

## Closed by the continuation pass

- Rust `jingwei-codegen` output is generated twice, byte-compared, imported, and registered through the production adapter.
- Official OpenAI Responses SDK streaming, reasoning, function-call aliases, continuation items, usage, and cancellation signal
  plumbing are automated; only the credential-dependent external call remains.
- Authenticated MCP Streamable HTTP is exercised against an official SDK server in a separate process.
- OTel SDK spans are exported over OTLP/HTTP to a real collector and inspected for trace ID, parent span, and redaction.
  This closes only the Runtime-side exporter gate, not the Rust daemon cross-runtime gate.
