# Task 05 handoff: Tauri native semantic workbench

## Result

Task 05 now provides a self-contained Tauri 2 + React 19 + TypeScript desktop workbench under `app/`. The application implements the task-book's four-region information architecture, semantic Canvas2D schematic/PCB rendering, selection and cross-probe, preview-to-commit editing, revision-aware undo/redo, AI patch review, durable runtime/tool/approval projection, reload recovery, and a constrained native shell.

The checked-in build is intentionally a development mock. It displays `VERSIONED MOCK · NOT REAL DAEMON EVIDENCE` in the main window, exposes separate Mock Runtime and Mock JDP connection labels, and keeps Production Release blocked. No fixture, screenshot, test, or package is presented as evidence that the real Runtime, JDP daemon, EDA kernel, solver, ERC/DRC engine, or manufacturing flow has run.

## Implemented scope

- Native Tauri window with top status bar, project/session tree, central editor, inspector, bottom work area, and approval drawer.
- A UI-owned `RuntimeClient` / `DesignClient` facade boundary. React components do not import runtime, daemon, or EDA-kernel internals.
- Versioned `v0.1` mock clients and traceable UI fixtures, isolated under `app/src/mocks/v0.1` and `testdata/ui/v0.1`.
- Canvas2D semantic display lists with viewport culling, deterministic hit testing, selection, finding overlays, cross-probe, drag preview, and explicit commit.
- Schematic wire creation and symbol move; PCB footprint move, track routing, and via placement; property edit and lock-policy change.
- Draft/current/candidate revision separation. Successful edits advance only `draft`; `current` remains unchanged until a future kernel-owned promotion flow.
- Undo, redo, checkpoint, semantic diff hunks, per-hunk acceptance, protected-object rejection, and AI-origin provenance.
- Runtime timeline projection for agent/model/tool/approval/failure events, replay cursor recovery, approval target/effect display, and retry/create-session mock flows.
- Tauri-side controlled project picker, external HTTP(S) opening, credential-reference validation, sidecar manifest/hash/version/target validation, handshake parsing, health, crash, and unavailable-state reporting.
- Strict Tauri capability allowlist and CSP; no frontend shell/filesystem permission, no raw credentials in frontend contracts, and no production devtools.

## What is real and what is simulated

Real implementation in this task:

- Native Tauri shell and release executable.
- Canvas2D display-list construction, culling, hit testing, render scheduling, and interaction state machine.
- UI facade contracts, reducer boundaries, recovery rules, security validation, and test suite.
- Local desktop interactions recorded in the evidence directory.

Versioned simulation:

- Runtime events, model/tool attempts, approval records, solver output, JDP responses, geometry, findings, rules, and semantic patches.
- `sessionStorage` persistence used only to emulate server-backed mock draft/event state across reload. Local selection, hover, and ghost preview are deliberately not restored.
- Sidecar manifest entries are `bundled: false`; the native shell reports the real sidecars as unavailable instead of silently substituting the mock clients.

## Acceptance summary

- Frontend: lint, typecheck, production build, 30 Vitest tests, and coverage command passed.
- Native shell: format check, Clippy with warnings denied, and 7 Rust tests passed.
- Performance fixtures: 10k and 100k primitive culling gates passed; the final 100k run culled to 3,168 visible primitives in 5.27 ms.
- Packaging: unsigned current-user NSIS package built successfully; the release executable was launched and exercised through Windows desktop control.
- Desktop checks: wire, symbol move, footprint move, via, track, finding cross-probe, lock, AI patch, undo/redo, approval timeline, native project picker, sidecar recheck, and reload recovery were exercised.

## Integration order

1. Task 01 should replace the mock runtime construction with an adapter implementing `RuntimeClient`, preserving durable event sequence and approval semantics.
2. Task 02 should replace the mock design construction with generated JDP bindings implementing `DesignClient`; transport and protocol types must stay outside React components.
3. Tasks 03 and 04 should provide display-list, query, preview, snap, edit-candidate, validation, and commit results through the daemon. The canvas remains a projection; the kernel transaction service owns revision commits.
4. Release engineering should bundle signed runtime/daemon sidecars, update the manifest with exact version/target/SHA-256 values, sign the installer, and repeat the native acceptance gates with non-mock evidence.

See [interfaces.md](interfaces.md), [known-gaps.md](known-gaps.md), [commands-and-results.md](commands-and-results.md), [artifacts.md](artifacts.md), and [changed-files.txt](changed-files.txt).
