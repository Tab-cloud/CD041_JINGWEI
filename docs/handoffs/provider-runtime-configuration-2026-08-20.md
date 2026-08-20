# Provider Runtime configuration handoff — 2026-08-20

## Outcome

The saved desktop model profile is now applied to the real Runtime rather than being used only by the connection-test path.
The previous `No provider is configured` failure is closed at the registry boundary: saving settings hot-applies a provider,
Runtime startup restores it before reporting readiness, and every Agent submission refreshes capabilities before admission.

## Protocols

- `openai-responses`: native OpenAI Responses API, including structured-output and multimodal capability declarations.
- `openai-chat-completions`: OpenAI-compatible Chat Completions streaming, including fragmented tool-call assembly,
  continuation history, usage collection, cancellation, and canonical Jingwei tool identity mapping.

Protocol selection is explicit. The UI warns that `api.minimaxi.com` uses Chat Completions and that the documented
`MiniMax-M3` model ID is case-sensitive; it does not silently retry a request through a different protocol.

## Secure apply path

1. React saves the non-secret profile and optional new key through the dedicated Tauri command.
2. Tauri reads the key from the operating-system credential store and calls `provider.configure` over the per-launch
   authenticated Runtime named pipe.
3. Runtime validates the endpoint, model, key shape, timeout, and protocol, constructs the complete adapter, then atomically
   replaces the registry entry.
4. Tauri zeroizes the serialized sensitive frame and key. The sidecar zeroes the input frame and scrubs the parsed key field.
5. The response exposes capabilities only; it cannot contain an API key, response content, or upstream error body.
6. Credential deletion calls `provider.clear`. An invalid replacement leaves the previous working provider intact.

The generic WebView `runtime_rpc` allowlist does not expose `provider.configure`, `provider.clear`, or `provider.test`.

## UI behavior

- The model drawer offers both protocols, a persistent `测试连接` action, sanitized protocol/host/model/latency evidence,
  MiniMax compatibility guidance, and immediate Runtime apply feedback.
- The top-bar Run button and Agent composer are disabled until Runtime reports at least one provider.
- Submission re-reads Runtime capabilities, preventing stale frontend state from producing a misleading failed run.
- If Runtime is unavailable during save, the configuration remains saved and is applied automatically on the next startup.

## Automated verification

- Runtime workspace: 53 passed, 1 environment-gated commercial live contract skipped, 0 failed; all packages built.
- Chat Completions adapter: text streaming, fragmented tool calls, continuation history, usage, endpoint path, and secret
  non-return assertions passed.
- Authenticated Runtime pipe: configure, capability exposure, atomic rejected replacement, clear, and secret non-return
  assertions passed against the real Rust daemon/TypeScript sidecar pair.
- Workbench: ESLint, TypeScript/Vite production build, and 50 Vitest tests passed.
- Rust workspace and Tauri: all tests passed; formatting and Clippy with warnings denied passed.

## Remaining installed-app gate

The package must still be rebuilt from a clean committed revision, installed over the existing 0.1.0 application, and checked
with the user-owned credential through both `测试连接` and one minimal Agent request. Do not claim that paid endpoint gate from
the environment-gated automated test alone. The outcome of that installed-app gate belongs in package evidence rather than
being backfilled into this implementation handoff.
