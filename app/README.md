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

The Windows package now pins and bundles real Runtime and daemon sidecars. The native supervisor verifies SHA-256 plus
protocol/version/target handshakes, passes bootstrap secrets only over stdin, starts daemon before Runtime, and kills both
children on application exit. The React workbench still uses versioned mock data adapters, so sidecar process health is never
presented as real schematic, PCB, ERC/DRC, or manufacturing evidence.

## Checks

```text
pnpm install --frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test
pnpm build
pnpm build:sidecars
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets --all-features
pnpm tauri build --bundles nsis
pwsh -NoProfile -File ../scripts/verify-windows-sidecars.ps1
```

Integration and acceptance evidence is in `../docs/handoffs/task-05/`, `../docs/handoffs/integration-2026-08-19.md`, and
`../testdata/sidecars/`.
