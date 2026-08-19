# Task 05 handoff: Tauri native semantic workbench

## Result

Task 05 now provides a self-contained Tauri 2 + React 19 + TypeScript desktop workbench under `app/`. The application implements the task-book's four-region information architecture, semantic Canvas2D schematic/PCB rendering, selection and cross-probe, preview-to-commit editing, revision-aware undo/redo, AI patch review, durable runtime/tool/approval projection, reload recovery, and a constrained native shell.

The top bar now also opens a native model-provider drawer. It separates `OPENAI_API_KEY` / `JINGWEI_OPENAI_MODEL` environment status from the local desktop profile, accepts provider endpoint/model/API Key input, stores the key in Windows Credential Manager, and never reads the key back into React. With the current missing environment variables and no saved local model/key, the commercial OpenAI endpoint is visibly blocked and not executed.

The checked-in UI is intentionally a development mock. It displays `VERSIONED MOCK · NOT REAL DAEMON EVIDENCE` in the main window, exposes separate Mock Runtime and Mock JDP data labels, and keeps Production Release blocked. The integration branch additionally bundles real Runtime and daemon processes, but their green process-health indicators remain separate from the mock design-data ribbon.

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
- Model/provider settings with HTTPS/model/key validation, non-secret profile persistence, Windows Credential Manager storage, key deletion, environment-source status, and explicit live-test readiness.
- Strict Tauri capability allowlist and CSP; no frontend shell/filesystem permission, no raw credentials in frontend contracts, and no production devtools.
- Product branding corrected to `精卫 Agent`; the app, generated platform icons, and rebuilt Windows development package use the 精卫衔石、海浪与电路翼标志。

## What is real and what is simulated

Real implementation in this task:

- Native Tauri shell and release executable.
- Canvas2D display-list construction, culling, hit testing, render scheduling, and interaction state machine.
- UI facade contracts, reducer boundaries, recovery rules, security validation, and test suite.
- Local desktop interactions recorded in the evidence directory.
- On the integration branch, pinned Windows Runtime/daemon binaries, supervised startup, versioned handshake, installed resource resolution, and exit cleanup.

Versioned simulation:

- Runtime events, model/tool attempts, approval records, solver output, JDP responses, geometry, findings, rules, and semantic patches.
- `sessionStorage` persistence used only to emulate server-backed mock draft/event state across reload. Local selection, hover, and ghost preview are deliberately not restored.
- The workbench's `RuntimeClient` and `DesignClient` are still versioned mocks. Real sidecar process health does not substitute for those data adapters.

## Acceptance summary

- Frontend: lint, typecheck, production build, and 40 Vitest tests passed.
- Native shell: targeted format check, Clippy with warnings denied, and 13 Rust tests passed, including an isolated Credential Manager round trip.
- Performance fixtures: 10k and 100k primitive culling gates passed; the final 100k run culled to 3,168 visible primitives in 5.27 ms.
- Packaging: unsigned current-user NSIS package built successfully, installed to an isolated smoke directory, and exercised through Windows desktop control.
- Desktop checks: wire, symbol move, footprint move, via, track, finding cross-probe, lock, AI patch, undo/redo, approval timeline, native project picker, sidecar recheck, and reload recovery were exercised.
- The rebuilt release executable was opened after the provider change; its model drawer showed both environment variables absent, desktop configuration incomplete, and the commercial endpoint test skipped. No key was entered and no endpoint request was made.

## Integration order

1. The real Runtime sidecar is bundled; the next integration slice must replace the workbench's mock runtime construction with a transport adapter implementing `RuntimeClient`, preserving durable event sequence and approval semantics.
2. Formal Rust-generated JDP bindings and a real daemon transport are present; the next slice must replace the mock design construction with a `DesignClient` adapter while keeping transport and protocol types outside React components.
3. Tasks 03 and 04 should provide display-list, query, preview, snap, edit-candidate, validation, and commit results through the daemon. The canvas remains a projection; the kernel transaction service owns revision commits.
4. Release engineering must sign the sidecars and installer, add update/rollback and provenance gates, and repeat installed-app acceptance with real design data.

See [interfaces.md](interfaces.md), [known-gaps.md](known-gaps.md), [commands-and-results.md](commands-and-results.md), [artifacts.md](artifacts.md), and [changed-files.txt](changed-files.txt).
