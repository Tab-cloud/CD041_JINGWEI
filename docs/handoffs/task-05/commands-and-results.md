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
- Vitest: 11 files, 30 tests passed, 0 failed.
- Vite production build: 64 modules transformed; main JavaScript 275.84 kB, 86.51 kB gzip.
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
- Rust tests: 7 passed, 0 failed.
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
size: 2,698,659 bytes
SHA-256: AB616454BA85308D63EE94C79B42E444E244937C612033E430426DBFF852E2C4
signature: NotSigned
```

The installer was built but not installed. The release executable itself was launched for the desktop acceptance checks. This remains a development artifact, not a Production Release.

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

- 44 TypeScript/TSX/Rust source files under the frontend/native source roots were scanned; none exceeded 600 lines. The largest React component was 122 lines.
- All 11 task JSON files parsed successfully.
- Scoped trailing-whitespace, common credential-pattern, and emoji scans passed.
- Generated dependency/build directories are ignored.
- Tauri capabilities expose only dialog opening; there is no frontend shell or filesystem capability.
- External links are restricted to HTTP(S), credential references are identifier-only, and manifest entries require version/target/hash validation before process launch.
- Devtools are disabled in the production Tauri configuration.
