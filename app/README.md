# Jingwei native workbench

This directory is the task-05 Tauri 2 desktop workbench. It is intentionally self-contained so it can be tested before the shared Runtime/JDP packages are merged.

## Layout

- `src/clients/`: UI-only facade contracts and structured client errors.
- `src/mocks/v0.1/`: versioned Runtime/JDP development servers and fixtures.
- `src/canvas/`: Canvas2D display-list culling, hit geometry, rendering, and benchmark tests.
- `src/components/`: workbench regions; bottom-panel implementations live in `components/bottom/`.
- `src/store/`: reducer and recovery cursor/view state.
- `src/utils/`: narrow Tauri command bridge.
- `src/styles/`: tokens, layout, panel, component, and responsive styles.
- `src-tauri/`: Rust shell, capability policy, sidecar manifest, icons, and Windows bundle configuration.
- `../testdata/ui/v0.1/`: transport-independent UI scenario and benchmark metadata.

Generated `node_modules/`, `dist/`, `coverage/`, and Rust `target/` directories are ignored. Standard multi-platform icon variants are kept together under `src-tauri/icons/`; application code does not use emoji as icons.

## Development mode

The current build uses contract version `0.1` mock clients. In Tauri only, the mock server snapshot is kept under versioned `sessionStorage` keys so a WebView reload can exercise server-backed draft and durable-cursor recovery. Browser previews do not persist mock server state. This mechanism must be removed with the mock transport and is not a JIR store.

The packaged sidecar manifest marks both runtime and daemon as `bundled: false`. The shell reports them as unavailable and never launches placeholder executables.

## Checks

```text
pnpm install --frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets --all-features
pnpm tauri build --bundles nsis
```

Integration and acceptance evidence is in `../docs/handoffs/task-05/`.
