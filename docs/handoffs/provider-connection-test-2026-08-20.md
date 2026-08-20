# Provider connection test handoff — 2026-08-20

## Outcome

The model settings drawer now contains a real “测试连接” action. It tests only saved configuration, runs through the
authenticated desktop-to-Runtime pipe, and returns sanitized evidence. React never receives an API key, response content,
authorization header, or upstream error body.

## Contract and ownership

1. React calls `model_configuration_test` without parameters.
2. Tauri prefers the saved desktop profile plus Windows Credential Manager secret; a complete
   `OPENAI_API_KEY`/`JINGWEI_OPENAI_MODEL` environment pair is the fallback.
3. Tauri serializes `provider.test` directly to the per-launch authenticated Runtime named pipe and zeroizes the mutable
   request buffer and credential string after use.
4. Runtime validates HTTPS, provider, model, API-key shape, and a 5–60 second timeout, then performs one streaming Responses
   request with retries and provider-side storage disabled.
5. The sidecar clears the consumed binary frame and replaces the in-memory request secret field after dispatch.
6. The result contains only status, provider/protocol, endpoint host, model, latency, timestamp, optional usage, and a
   structured sanitized failure.

The generic frontend `runtime_rpc` allowlist does not expose `provider.test`; only the dedicated Tauri command can supply its
credential-bearing internal request. JDP and `jingwei-daemon` are not involved.

## UI behavior

- `READY · 未执行`: saved desktop configuration or complete environment configuration is available.
- `TESTING · 请求中`: a bounded request is in flight; settings controls are disabled.
- `PASS · 已连接`: valid Responses terminal event received; host, model, latency, time, and usage metadata are shown.
- `FAILED · 已执行`: authentication, endpoint, rate-limit, timeout, network, request-compatibility, service, or protocol
  failure is shown without raw provider content.
- `BLOCKED`/`SKIPPED`: missing configuration or unavailable Runtime; no request is attempted.
- Any unsaved endpoint/model/key edit disables testing and instructs the user to save first.

## Verification

- Root `pnpm check`: 47 passed, 1 credential-gated commercial live contract skipped, 0 failed; all packages built.
- App: ESLint, TypeScript, Vite production build, and 47 Vitest tests passed.
- Tauri: formatting, 13 Rust tests, and Clippy with warnings denied passed.
- Runtime `provider.test`: success, sanitized 401 failure, unsafe endpoint rejection, authenticated pipe dispatch, and secret
  non-return assertions passed.
- Light and dark model-drawer layouts were visually checked at the desktop breakpoint.
- Alpha packaging removes any previous `windows-installed-app-gate.json`; installed-app evidence is build-specific and must
  not survive into a package produced from a different installer hash or Git commit.

## Truthful remaining gate

No approved commercial credential was provided to the automated build environment, so this delivery does not claim a paid
OpenAI or OpenAI-compatible endpoint passed. The installed user can initiate that test explicitly from the drawer. The
unsigned Alpha release and the broader Production Release gates remain unchanged.

## 0.1.1 stale-build correction

The user screenshot that followed this handoff was produced by `D:\Tools\Jingwei Agent Dev\jingwei-workbench.exe`, an older
`com.jingweiagent.workbench.dev` build that still displayed `MOCK 0.1 · DEV`. Release 0.1.1 therefore adds three explicit
guards: a visible `APP 0.1.1` badge, a uniquely named installer, and one-way migration of the validated non-secret provider
profile from the legacy Dev configuration sibling. The Windows credential remains in Credential Manager and is never copied
to a file. When the desktop profile and credential are available, missing process environment variables are now labelled
“可选 · 未使用” instead of implying that duplicate environment configuration is required.
