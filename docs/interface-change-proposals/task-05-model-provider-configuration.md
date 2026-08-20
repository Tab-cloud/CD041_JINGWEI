# Task 05 interface proposal: model provider configuration and credential broker

Status: desktop configuration and one-shot Runtime connection test implemented; broader profile lifecycle deferred.

## Problem

The current development environment has neither `OPENAI_API_KEY` nor `JINGWEI_OPENAI_MODEL`. The commercial OpenAI endpoint live test is therefore explicitly skipped. A production workbench still needs a user-facing way to configure the provider without placing secrets in project files, frontend storage, logs, JDP envelopes, or ordinary Runtime events.

## Implemented desktop boundary

The Tauri shell owns local configuration capture:

- `model_configuration_get` returns the non-secret provider profile, boolean environment-variable presence, and boolean credential-store state.
- `model_configuration_save` validates the HTTPS endpoint and model ID, writes the API key to Windows Credential Manager, then writes only `provider`, `baseUrl`, `model`, and opaque `credentialRef` metadata to the application config directory.
- `model_configuration_delete_credential` removes only the credential. The API key is never returned to React after submission.
- `model_configuration_test` accepts no frontend parameters. Tauri resolves the saved profile and OS credential (or the two
  approved environment variables), sends them through the authenticated local Runtime pipe, and actively clears mutable
  secret buffers after use.
- Browser preview has no credential-write fallback and cannot claim that a key was persisted.

The initial provider ID is `openai-responses`, with the default endpoint `https://api.openai.com/v1`. The UI may add additional providers later, but provider-specific credentials must continue to cross frontend and project boundaries only as opaque references.

The current native backend is Windows-first. macOS Keychain and Linux Secret Service adapters remain required before those targets can save credentials. Until they exist, those builds must report the credential store as unavailable and must not use a plaintext fallback.

## Implemented Runtime test contract

The desktop bridge exposes one dedicated operation; generic workbench Runtime RPC cannot invoke it:

```text
model_configuration_test() -> ProviderConnectionTestResult
  -> authenticated local pipe provider.test(ProviderConnectionTestInput)
  -> one bounded OpenAI Responses request
```

`ProviderConnectionTestInput` is an internal, transport-only contract containing `provider`, `baseUrl`, `model`, `apiKey`,
and a bounded timeout. The API key is never accepted from React: the desktop credential broker resolves it only for the
authorized Runtime process over the per-launch authenticated named pipe. Request buffers and object references are scrubbed
after dispatch. JDP and the EDA daemon never receive or resolve provider credentials.

`ProviderTestResult` must be typed and evidence-oriented:

```text
status: passed | failed
provider: openai-responses
protocol: openai-responses
model: string
endpointHost: string
latencyMs: number
usage?: { inputTokens, outputTokens }
error?: { code, summary, retryable, httpStatus? }
testedAt: string
```

The result must not include request authorization headers, raw provider responses, prompt content, or credential material. A skipped result is a valid result when credentials, model, network authorization, or Runtime capability are absent.

## Acceptance

- Missing environment variables are visible independently; they are not reported as a network failure.
- A desktop profile plus OS-stored key can satisfy configuration readiness without mutating process environment variables.
- Saving a key leaves no secret in application JSON, project artifacts, `localStorage`, `sessionStorage`, logs, telemetry, crash metadata, or UI readback.
- The UI never performs a commercial endpoint request directly. Only the Runtime `provider.test` operation may execute the live test.
- Runtime unavailable or test capability absent produces `skipped` or `blocked` with a truthful reason, never a false success.
- Credential deletion is idempotent and last-used credentials are not cached by the frontend.

Full Runtime-owned `provider.profile.get/apply/delete` lifecycle operations remain a future multi-profile feature. They are
not required for this single local desktop profile and must preserve the same credential boundary if added.

## Native reference

- Microsoft `CredWriteW`: https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew
- Microsoft `CredReadW`: https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credreadw
- Microsoft `CredDeleteW`: https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-creddeletew
