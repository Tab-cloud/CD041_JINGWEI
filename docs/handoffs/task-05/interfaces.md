# Task 05 interfaces

## UI facade boundary

The only UI-facing service contracts are in `app/src/clients/contracts.ts`:

- `RuntimeClient`: connect, disconnect, start session, subscribe/replay events, respond to approvals, retry a failed step, and report connection state.
- `DesignClient`: connect, query schematic/PCB display lists, hit-test, cross-probe, preview/commit edit candidates, update properties and edit policy, undo/redo/checkpoint, request/accept semantic patch hunks, and report connection state.

React components consume controller state and commands. They do not import task-01 runtime internals, task-02 protocol/daemon internals, or task-03/task-04 EDA implementation modules.

## Versioning and error shape

- The mock facade version is `0.1` and is visible in the window.
- Client failures use stable error codes plus message, retryability, and details. UI notifications and connection badges preserve the distinction between unavailable, version mismatch, crash, and semantic rejection.
- Runtime events preserve sequence numbers and timestamps. Recovery resumes from the last durable sequence instead of treating timeline animation as source-of-truth state.
- Design responses always carry revision identity. Preview is ephemeral; commit advances draft and does not mutate current.

## Replacement map

1. Task 01 integration: provide a `RuntimeClient` adapter around the public runtime package, then remove `MockRuntimeClient` and its mock persistence from the controller construction path.
2. Task 02 integration: generate or import the official JDP TypeScript binding in an infrastructure adapter that implements `DesignClient`. Do not leak generated transport types into React components.
3. Task 03 integration: map logical schematic display lists, object IDs, cross-probe, findings, preview, semantic edits, and transaction results to the existing facade.
4. Task 04 integration: map PCB display lists, footprints, tracks, vias, board regions, DRC/DFM results, candidate revisions, and solver runs through the daemon-owned facade.
5. Native integration: set both sidecar manifest entries to `bundled: true`, record the exact protocol version, target triple, file name, and SHA-256 digest, and bundle the matching signed binaries. The existing handshake and health checks remain mandatory.

## Interface-change proposal status

No interface-change proposal is included with task 05. The local TypeScript contracts are a UI projection over the task-book concepts and do not redefine the normative wire protocol. If the generated JDP bindings cannot satisfy this facade without changing protocol semantics, the integration owner must create a proposal before changing shared contracts.
