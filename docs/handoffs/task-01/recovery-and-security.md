# Recovery and security evidence

## Kill-point handling

1. Durable input, no run: input remains pending after SQLite reopen.
2. Provider partial stream: attempt becomes `discarded`; buffered text/reasoning/tool calls do not enter messages.
3. Tool recorded, handler not started: recovery classifies it safe only for same-call reauthorization.
4. Approval waiting: request remains queryable and rejectable after runtime reopen.
5. JDP committed, response lost: adapter calls `reconcile`; the test proves only one `invoke`.
6. MCP/external state unknown: execution becomes `unknown`/needs-reconciliation and is not retried.
7. UI consumer disconnect: history and live sequence bridge produces no gap or duplicate.
8. Schema migration failure: store construction fails closed before model execution.

The machine-readable matrix is `testdata/runtime/fault-injection-cases.json`.

## Security negatives

- A visible tool with a deny policy never reaches its handler.
- A project allow cannot override a matching deny.
- Specialists cannot own root runs and cannot execute root-only tools.
- MCP instructions, descriptions, annotations, and output remain untrusted.
- MCP tool effects remain worst-case despite `readOnlyHint` or similar metadata.
- MCP endpoints cannot contain credentials; remote credentials are references resolved out of band.
- Approval input and tool output redact secret-bearing fields and token patterns.
- Scoped approval cannot escape session, permission, resources, or expiry.
- Approval request/decision persistence failure remains fail-closed.
- OTel allows identifiers/status/timing only; prompt/design/secret/output attributes are discarded.
- Oversized tool output is stored as a hashed artifact and only a bounded reference reaches the model.
- Conflicting resource claims fence writes while disjoint/read-only claims can proceed.
