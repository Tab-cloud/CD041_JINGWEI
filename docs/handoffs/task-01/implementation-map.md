# Implementation map

## Package boundaries

| Package | Ownership | Key public API |
| --- | --- | --- |
| `agent-protocol` | Versioned runtime/JDP wire types and typed errors | `RuntimeClient`, `JdpClient`, `DurableEvent` |
| `agent-events` | Append, aggregate sequence, replay/live bridge, projection, migration | `DurableEventBus`, `SqliteEventStore` |
| `agent-policy` | Layered authorization, root authority, durable approvals | `PolicyEngine`, `ApprovalService` |
| `agent-tools` | Canonical registry, settlement, execution/artifact stores, generated JDP catalog, resource leases | `ToolSettlementPipeline`, `registerRustGeneratedJdpCatalog` |
| `agent-mcp` | Per-server MCP isolation and Streamable HTTP transport boundary | `McpHost`, `HttpMcpTransport` |
| `agent-observability` | OTel spans, OTLP export, trace propagation, redaction and attribute allowlist | `RuntimeTracer`, `createOtlpTracing` |
| `agent-runtime` | Session/run/provider/loop/context/recovery/specialists/client composition | `createRuntime`, `AgentRuntimeService`, `OpenAIResponsesProvider` |

Every program file has a single responsibility and remains below the task's 600-line TypeScript threshold. Migrations,
fixtures, tests, and handoff evidence live next to their owning package or under their designated task directories.

## Eval-derived behavior mapping

No Eval file was copied or moved. The implementation is a brand-neutral rewrite of tested behavior from the following
reference areas:

| Eval reference | New implementation |
| --- | --- |
| `runtime/session-command-queue.ts` and durable command tests | `runtime-repository.ts`, `run-coordinator.ts`, `runtime-service.ts` |
| `runtime/agent-loop.ts` and tool execution loop | `agent-loop.ts`, `provider.ts`, canonical settlement |
| `runtime/agent-authority.ts` | `agent-policy/authority.ts`, `specialist-runner.ts` |
| `runtime/tool-approval.ts` | `agent-policy/approval.ts` with narrowed scoped grants |
| `store/tool-execution.ts` | `agent-tools/execution-store.ts` and `settlement.ts` |
| `mcp/manager.ts` | `agent-mcp/host.ts` with explicit allowlist and worst-case effects |

Jingwei-specific strengthening includes aggregate event sequence/replay, stale tool identities, output artifacting, generated
JDP tools, OTel propagation, process owner lease, schema fail-closed behavior, and unknown-side-effect quarantine.

The Rust schema remains authoritative: tests launch `jingwei-codegen`, verify byte-for-byte reproducibility, import the
generated TypeScript constants, and feed the generated tool catalog into the canonical registry. Generated artifacts stay in
the ignored `target/` test area because `bindings/typescript/**` belongs to task 02.

## Design-data boundary

The runtime stores only control-plane facts and references. It has no JIR schema, operation journal, revision pointer, geometry,
connectivity, release signature, or direct workspace design writer. Generated JDP tools are the only design-action boundary.
