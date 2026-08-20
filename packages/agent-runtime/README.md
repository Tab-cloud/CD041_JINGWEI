# @jingwei/agent-runtime

The brand-neutral durable control plane. It owns sessions, admitted inputs, root/specialist runs, provider attempts,
single-session coordination, context projection, cancellation, owner leases, and restart reconciliation. It never writes JIR
or workspace design state; all design actions cross the `JdpClient` boundary.

`createRuntime()` requires an explicit JDP client and defaults policy to deny. `MockJdpClient` and `FakeProvider` are exported
for contracts and fault injection only; neither is silently selected for production.

`OpenAIResponsesProvider` and `OpenAIChatCompletionsProvider` are the production commercial-provider adapters. Both use the
official OpenAI JavaScript SDK, require an explicitly selected protocol plus credential/model limits, disable hidden SDK
retries, and stream text/tool calls while preserving function-call history across continuation turns. Responses additionally
supports structured output and multimodal capability declarations. Chat Completions is available for compatible endpoints
such as MiniMax that do not expose `/responses`. The optional environment-based live contract runs only when both
`OPENAI_API_KEY` and `JINGWEI_OPENAI_MODEL` are present.

`testProviderConnection()` backs the desktop `provider.test` RPC. It executes one bounded request through the explicitly
selected Responses or Chat Completions protocol with SDK retries disabled and provider-side storage disabled, then returns only
endpoint host, model, protocol, latency, token usage, timestamp, and a structured failure classification. It never returns
response content, authorization headers, upstream error bodies, or credential material.

The desktop calls `provider.configure` after saving settings and during Runtime startup. The Runtime constructs a complete
adapter before atomically replacing the current registry entry, so an invalid replacement cannot erase a working provider.
Deleting the operating-system credential calls `provider.clear`. Credential-bearing frames are authenticated, bounded,
zeroized on the native side, scrubbed in the sidecar, and never exposed through the generic WebView Runtime RPC allowlist.
