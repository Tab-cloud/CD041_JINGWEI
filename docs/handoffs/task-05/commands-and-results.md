# Task 05 commands and results

Environment date: 2026-08-19, Asia/Shanghai.

## Toolchain

```text
Node.js v24.19.0
pnpm 11.19.0
rustc 1.97.1 (8bab26f4f 2026-07-14)
cargo 1.97.1 (c980f4866 2026-06-30)
Tauri 2
React 19
```

The app has its own `pnpm-workspace.yaml` and Rust `[workspace]` boundary so task-05 checks are isolated from concurrently created root workspaces.

## Frontend checks

```text
pnpm install --frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test -- --reporter=verbose
pnpm test:coverage
pnpm build
```

Results:

- Install, lint, typecheck, coverage command, and production build: exit 0.
- Vitest: 13 files, 36 tests passed, 0 failed.
- Vite production build: 66 modules transformed; main JavaScript 284.93 kB, 89.35 kB gzip.
- Coverage was collected as a diagnostic and has no release threshold in this task. Overall statement coverage was 36.45%, so it is not claimed as comprehensive coverage.

## Native checks

```text
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml \
  --all-targets --all-features -- -D warnings
cargo test --manifest-path app/src-tauri/Cargo.toml
```

Results:

- Rust format check: exit 0.
- Clippy with warnings denied: exit 0.
- Rust tests: 12 passed, 0 failed. The suite includes an isolated Windows Credential Manager write/read/delete round trip that uses a unique test target and cleanup guard.
- The test-link step emitted one localized MSVC `linker_messages` warning while creating the DLL import library. Clippy with `-D warnings` still passed, and the warning did not fail or skip any test.

## Performance fixtures

The measured tests build synthetic semantic display lists, cull them against a viewport, and assert the bounded interaction budget. These are deterministic UI-engine fixtures, not real daemon, EDA-kernel, or hardware FPS telemetry.

```text
10k:  build 2.01 ms; cull 1.44 ms; 3,168 visible
100k: build 21.58 ms; cull 5.27 ms; 3,168 visible
```

Both performance tests passed. Hit-test and preview tests also passed, but no end-to-end 60 FPS/P95 claim is made.

## Packaging

```text
pnpm tauri build --bundles nsis
```

Result: exit 0. Tauri reran the production frontend build, compiled the release binary, and produced a current-user NSIS installer.

Package evidence:

```text
Jingwei Agent Dev_0.1.0_x64-setup.exe
size: 2,713,792 bytes
SHA-256: 5A22EBFC9990C4FD70A20EA1E0EA91D873C973959DE55E103031C3B335D6CF6E
signature: NotSigned
```

The installer was built but not installed. The earlier pre-brand-refresh release executable was launched for the desktop interaction
acceptance checks; the current rebuilt executable was verified by extracting its embedded icon, but the full interaction matrix was
not repeated after this branding-only change. This remains a development artifact, not a Production Release.

## 精卫 branding and icon refresh

The Chinese product name was corrected from `经纬 Agent` to `精卫 Agent` in the product baseline, decision log, HTML title,
native window title, and workbench brand. The new icon is based on 精卫填海 rather than longitude/latitude or weaving imagery:
a determined bird carries an amber stone over sea waves, with restrained PCB traces integrated into one wing.

```text
pnpm exec tauri icon src-tauri/icons/app-icon.svg --output src-tauri/icons
pnpm run typecheck
pnpm run lint
pnpm run test
pnpm run build
pnpm exec tauri build --bundles nsis
```

Results: icon generation passed; typecheck and lint passed; the latest run has 13 test files / 36 tests passed; the production frontend build and
unsigned NSIS development bundle completed. The extracted executable icon is recorded as
`docs/handoffs/task-05/evidence/jingwei-logo-exe-icon.png`.

## Model provider and credential configuration

The environment was checked immediately before the package gate:

```text
OPENAI_API_KEY=absent
JINGWEI_OPENAI_MODEL=absent
```

Results:

- The frontend drawer shows environment and desktop sources independently and displays both variables as `未设置`.
- The commercial endpoint card reports `SKIPPED · 未执行` because the real Runtime sidecar is unavailable. No OpenAI endpoint request was issued.
- React never receives a saved key. The non-secret JSON profile contains only provider, HTTPS base URL, model ID, schema version, and opaque credential reference.
- Windows Credential Manager is used through the native Rust adapter. Invalid URLs/models/key shapes are rejected, and non-Windows builds fail closed until platform adapters exist.
- `cargo test --offline` passed 12 tests; `cargo clippy --offline --all-targets -- -D warnings` and formatting checks passed.
- The first `tauri dev` attempt exposed a Windows file-watcher collision on `src-tauri/target`; Vite now explicitly ignores that build directory, and the next dev launch completed.
- Both the dev window and rebuilt release executable were inspected through Windows desktop control. The settings drawer, missing-variable state, and skipped-live-test state were visible in each.

## Native desktop acceptance

Windows desktop control was used against `app/src-tauri/target/release/jingwei-workbench.exe`, not a browser preview.

Observed results:

- The native title, four-region layout, mock banner, current/candidate/draft revisions, release blockers, and real sidecar unavailable states were visible.
- Sidecar recheck retained explicit unavailable reasons because the development manifest has `bundled: false`.
- Native project folder picker opened and was canceled without changing the project.
- Schematic wire commit advanced draft `011 -> 012`; symbol move advanced `012 -> 013`.
- PCB footprint move advanced `013 -> 014`; via placement advanced `014 -> 015`; track routing advanced `015 -> 016`.
- Each edit notification explicitly stated that current revision was unchanged.
- DRC finding selection produced one selected object and related cross-probe objects.
- Lock-policy edit, AI patch review, protected `J1` hunk rejection, undo/redo, checkpoint, approval target/effect, and replay sequence were exercised.
- Reload restored the mock server-backed draft and replay cursor while discarding local ghost state. The restored view showed `replay caught-up · seq 6` and `16 / 16 primitives`.

## Source and security hygiene

- 50 TypeScript/TSX/Rust source files under the frontend/native source roots were scanned; none exceeded 600 lines. The largest React component was 215 lines.
- All 11 task JSON files parsed successfully.
- Scoped trailing-whitespace, common credential-pattern, and emoji scans passed.
- Generated dependency/build directories are ignored.
- Tauri capabilities expose only dialog opening; there is no frontend shell or filesystem capability.
- External links are restricted to HTTP(S), credential references are identifier-only, and manifest entries require version/target/hash validation before process launch.
- API keys are limited to the native command boundary, cleared from transient Rust/React state after use, stored in Windows Credential Manager, and never persisted in project/browser storage or returned by configuration reads.
- Devtools are disabled in the production Tauri configuration.
