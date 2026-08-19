# Source provenance

## Reference checkout

- Repository: `D:\Projects\CD041_LUBAN\embedded-agent`
- Commit: `2b2e29154168360f5a06410023a6465dadec2653`
- Commit date: 2026-08-19T18:08:01+08:00
- Working tree before reference tests: clean

Reviewed reference files include the agent loop, tool execution, authority, approval, session command queue, ToolExecution
store, MCP manager, and their focused tests named in `test-report.md`.

## Copy/move statement

- Moved from Eval: none.
- Copied verbatim from Eval: none.
- Rewritten from behavior/contracts: session admission/coordinator, authority, approval, tool settlement, recovery, and MCP host.
- Newly created for Jingwei: public runtime/JDP protocol, aggregate events, generated JDP adapter, artifact bounding, OTel layer,
  resource leases, specialist tool wrapper, fixtures, migrations, and handoff evidence.
- Eval source repository modified: no.

The implementation uses package APIs from Ajv, OpenTelemetry, the official OpenAI JavaScript SDK, and the official MCP
TypeScript SDK. No OpenCode/Codex source is copied or linked as a runtime dependency. Dependency license/NOTICE generation
remains an integration-release responsibility.
