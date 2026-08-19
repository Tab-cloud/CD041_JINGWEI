# Jingwei native workbench

This directory is the Tauri 2 desktop workbench. The production path uses authenticated local Runtime and daemon transports;
versioned fixtures remain available only through an explicit `?fixture=1` browser-development query.

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

## Runtime modes

The Windows package pins and bundles real Runtime and daemon sidecars. The native supervisor verifies SHA-256 plus
protocol/version/target handshakes, passes bootstrap secrets only over stdin, starts daemon before Runtime, and kills both
children on application exit. Runtime control uses its own authenticated named pipe; daemon JDP supports concurrent Runtime
and workbench clients. Tokens stay in Rust/native state and are not exposed to the WebView.

An ordinary browser preview deliberately fails with a clear transport-unavailable message. Append `?fixture=1` only when a
versioned UI fixture is specifically required. The packaged application never silently falls back to fixture data.

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
