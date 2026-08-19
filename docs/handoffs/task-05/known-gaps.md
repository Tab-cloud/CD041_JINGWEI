# Task 05 known gaps

## Production blockers

- The integration branch now bundles and supervises real Windows runtime and daemon sidecars, and their manifest hashes, versioned handshakes, installed resource paths, and exit cleanup have passed. However, the workbench controller still uses versioned `MockRuntimeClient` and `MockDesignClient` data adapters, so process health is not presented as real EDA design evidence.
- Runtime, JDP, solver, schematic/PCB geometry, snap, ERC/DRC, rules, AI patches, and manufacturing state are versioned fixtures. They are not evidence from tasks 01-04.
- The final NSIS package was installed to an isolated smoke directory and exercised as an installed app. It remains unsigned; release signing, rollback, update, and release provenance gates remain open.
- Production Release is intentionally blocked in the UI and cannot be promoted from this mock workbench.
- The Runtime credential broker and `provider.profile.test` operation are not implemented. `OPENAI_API_KEY` and `JINGWEI_OPENAI_MODEL` are absent in the verified environment, so the commercial OpenAI endpoint live test was explicitly skipped.
- Native credential persistence is Windows-first. macOS Keychain and Linux Secret Service adapters are not implemented; those builds must report unavailable and must never fall back to plaintext storage.

## Bounded MVP editing surface

The implemented minimum covers property edit, symbol/footprint move, wire/track creation, via placement, edit locks, undo/redo/checkpoint, and semantic patch review. The following task-book P0 operations are not exposed as complete production tools in this round:

- Place/remove symbol and disconnect wire.
- Footprint rotate/flip.
- Track/via move and delete.
- Board-region editing.
- Real snap/rule checks and kernel-owned transactional commit.

These must be completed through the real task-03/task-04 APIs; duplicating authoritative EDA rules in React would violate the architecture.

## Validation limits

- The 10k/100k benchmarks measure synthetic display-list culling, not instrumented frame pacing on representative real projects. There is no formal 60 FPS/P95 desktop telemetry result.
- Component/unit tests and direct desktop interaction were completed, but no automated desktop E2E harness is included.
- Attachment chips are a visual/runtime-event projection; project-artifact upload and content persistence are not implemented.
- Retry-with-change and request-user are represented in timeline state but do not yet have complete production action workflows.
- External-browser routing is unit-tested and native-code reviewed; the desktop smoke did not open a public URL because that would create an external side effect unrelated to the acceptance target.

## Mock recovery boundary

`sessionStorage` is used only inside the versioned mock server to emulate a durable server across a Tauri webview reload. It restores draft revision and replay cursor, while hover, selection, and ghost preview remain local and are discarded. Remove this emulator when the real Runtime/JDP transports are wired; the production UI must not treat browser storage as authoritative project state.
