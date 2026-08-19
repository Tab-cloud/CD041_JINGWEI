# @jingwei/agent-mcp

An isolated MCP host. Each server has its own transport, namespace, credential reference, explicit tool allowlist, timeout,
rate limit, and audit stream. Server instructions, descriptions, annotations, and results stay untrusted. Remote tools receive
worst-case effects; sampling and recursive agent loops are not exposed.

`HttpMcpTransport` implements MCP initialize/initialized negotiation, stateful session headers, protocol-version headers,
JSON and bounded SSE responses, cancellation, and credential-reference headers. Integration tests start an authenticated
official MCP SDK server in a separate Bun process and cover both a successful tool call and wrong-credential rejection.
