# @jingwei/agent-runtime

The brand-neutral durable control plane. It owns sessions, admitted inputs, root/specialist runs, provider attempts,
single-session coordination, context projection, cancellation, owner leases, and restart reconciliation. It never writes JIR
or workspace design state; all design actions cross the `JdpClient` boundary.

`createRuntime()` requires an explicit JDP client and defaults policy to deny. `MockJdpClient` and `FakeProvider` are exported
for contracts and fault injection only; neither is silently selected for production.

`OpenAIResponsesProvider` is the production commercial-provider adapter. It uses the official OpenAI JavaScript SDK,
requires credentials and model limits to be supplied explicitly, disables hidden SDK retries, defaults API-side response
storage to off, streams text/reasoning/tool calls, and preserves function-call history across continuation turns. The optional
live contract runs only when both `OPENAI_API_KEY` and `JINGWEI_OPENAI_MODEL` are present.

`testProviderConnection()` backs the desktop `provider.test` RPC. It executes one bounded Responses request with SDK
retries and provider-side storage disabled, then returns only endpoint host, model, latency, token usage, timestamp, and a
structured failure classification. It never returns response content, authorization headers, upstream error bodies, or
credential material.
