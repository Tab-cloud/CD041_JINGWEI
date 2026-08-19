# Task 05 interface proposal: model provider configuration and credential broker

Status: desktop configuration implemented; Runtime integration proposed.

## Problem

The current development environment has neither `OPENAI_API_KEY` nor `JINGWEI_OPENAI_MODEL`. The commercial OpenAI endpoint live test is therefore explicitly skipped. A production workbench still needs a user-facing way to configure the provider without placing secrets in project files, frontend storage, logs, JDP envelopes, or ordinary Runtime events.

## Implemented desktop boundary

The Tauri shell owns local configuration capture:

- `model_configuration_get` returns the non-secret provider profile, boolean environment-variable presence, and boolean credential-store state.
- `model_configuration_save` validates the HTTPS endpoint and model ID, writes the API key to Windows Credential Manager, then writes only `provider`, `baseUrl`, `model`, and opaque `credentialRef` metadata to the application config directory.
- `model_configuration_delete_credential` removes only the credential. The API key is never returned to React after submission.
- Browser preview has no credential-write fallback and cannot claim that a key was persisted.

The initial provider ID is `openai-responses`, with the default endpoint `https://api.openai.com/v1`. The UI may add additional providers later, but provider-specific credentials must continue to cross frontend and project boundaries only as opaque references.

The current native backend is Windows-first. macOS Keychain and Linux Secret Service adapters remain required before those targets can save credentials. Until they exist, those builds must report the credential store as unavailable and must not use a plaintext fallback.

## Proposed Runtime contract

Task 01 should add a credential broker and these versioned operations to the Runtime client boundary:

```text
provider.profile.get() -> ProviderProfileView
provider.profile.apply(profile: ProviderProfileInput) -> ProviderProfileView
provider.profile.test(profileId: string) -> ProviderTestResult
provider.profile.delete(profileId: string) -> DeleteResult
```

`ProviderProfileInput` contains `provider`, `baseUrl`, `model`, and `credentialRef`; it never contains a raw API key. The desktop credential broker resolves the reference only for the authorized Runtime process over an authenticated local channel. JDP and the EDA daemon must never receive or resolve provider credentials.

`ProviderTestResult` must be typed and evidence-oriented:

```text
status: succeeded | failed | skipped | blocked
provider: string
model: string
endpointHost: string
latencyMs?: number
httpStatus?: number
failureCode?: string
summary: string
evidenceRef?: string
testedAt?: string
```

The result must not include request authorization headers, raw provider responses, prompt content, or credential material. A skipped result is a valid result when credentials, model, network authorization, or Runtime capability are absent.

## Acceptance

- Missing environment variables are visible independently; they are not reported as a network failure.
- A desktop profile plus OS-stored key can satisfy configuration readiness without mutating process environment variables.
- Saving a key leaves no secret in application JSON, project artifacts, `localStorage`, `sessionStorage`, logs, telemetry, crash metadata, or UI readback.
- The UI never performs a commercial endpoint request directly. Only the Runtime `provider.profile.test` operation may execute the live test.
- Runtime unavailable or test capability absent produces `skipped` or `blocked` with a truthful reason, never a false success.
- Credential deletion is idempotent and last-used credentials are not cached by the frontend.

## Native reference

- Microsoft `CredWriteW`: https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew
- Microsoft `CredReadW`: https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credreadw
- Microsoft `CredDeleteW`: https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-creddeletew
