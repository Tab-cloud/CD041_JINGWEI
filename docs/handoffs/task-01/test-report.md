# Test report

Date: 2026-08-19. Toolchain: Bun 1.3.6, Node 24.19.0, pnpm 11.19.0, TypeScript 5.9.2.

## Jingwei runtime checks

| Check | Result |
| --- | --- |
| `pnpm install` | Passed; lockfile current; only `esbuild` build script explicitly allowed |
| `pnpm lint` | Passed |
| `pnpm typecheck` | Passed |
| `pnpm test` | 41 tests passed, 1 credential-gated live test skipped, 0 failed |
| `pnpm build` | Seven packages built with ESM and declaration output |
| `git diff --check` | Passed for all task 01 paths using intent-to-add visibility for new files |
| `pnpm audit --prod --registry=https://registry.npmjs.org` | No known vulnerabilities found |

The tests cover duplicate request IDs, session concurrency, coalesced wake, steer/queue ordering, provider partial discard,
native-tool capability refusal, cancellation, normal no-tool completion, tool visibility versus authorization, approval
durability failure, JDP reconciliation, MCP metadata attacks, output artifacting, root/specialist authority, resource conflicts,
event replay, secret redaction, OTel attributes, owner recovery, and migration failure.

New integration gates additionally cover official OpenAI SDK SSE and function-call continuation over loopback HTTP, Rust
`jingwei-codegen` reproducibility and catalog registration, an authenticated official MCP SDK server in a separate process,
and an OTLP/HTTP JSON export captured by a real loopback collector.

## Eval reference contracts

The unchanged Eval checkout was tested at commit `2b2e29154168360f5a06410023a6465dadec2653` with:

```text
bun test packages/core/test/session-command-queue.test.ts
         packages/core/test/tool-execution.test.ts
         packages/core/test/mcp.test.ts
         packages/core/test/multi-agent-runtime-boundaries.test.ts
         packages/core/test/governance-boundaries.test.ts
```

Result: 41 tests passed, 0 failed, 155 assertions.

## Real versus mock classification

| Area | Evidence type |
| --- | --- |
| SQLite persistence, reopen, WAL, migrations | Real file-backed SQLite through Bun |
| Session/run concurrency and cancellation | Real async runtime, scripted provider |
| Provider streaming/fault-domain fallback | `FakeProvider` for faults plus official OpenAI SDK against a real loopback SSE endpoint |
| Commercial provider live call | Explicitly skipped because `OPENAI_API_KEY` and `JINGWEI_OPENAI_MODEL` were absent |
| JDP binding/catalog | Real Rust `jingwei-codegen` process and generated catalog; invocation uses a recording client because daemon is absent |
| JDP invoke/reconcile recovery | `MockJdpClient` and a committed-response-loss contract double |
| MCP | Real authenticated Streamable HTTP server in a separate Bun process plus security doubles for list mutation |
| Runtime-side OTel | Real SDK batch processor and OTLP/HTTP exporter captured by a real loopback collector |
| Runtime → JDP → Rust daemon trace | **BLOCKED / not run**: no formal `jingwei-daemon`, `bindings/typescript/**`, or Rust trace-context receiver from task 02 |
| Network | Real loopback HTTP/process tests; no external endpoint or secret used |
| Eval source behavior | Real tests in the unchanged reference checkout |

Production dependency licenses were enumerated successfully. OpenTelemetry and the OpenAI JavaScript SDK are Apache-2.0;
Ajv, the MCP TypeScript SDK, Zod, and their relevant helpers are permissively licensed. Final product NOTICE generation
remains an integration-release gate.

The OTLP integration test proves that a Runtime `jdp.request` span can adopt supplied parent context, export through the real
OTLP/HTTP SDK path, and omit forbidden attributes. It does not start a Rust daemon and does not prove that a daemon handler
span joins the same trace. Cross-runtime acceptance requires both real spans in one collector payload with the daemon parent
equal to the Runtime `jdp.request` span ID.
