# Task 04 interfaces

## Consumed contract

`jingwei-pcb::LogicalNetlist` freezes `logical-netlist-v0` with source revision, component instances, exact footprint/pad definitions, pins, nets, constraints, and evidence references. `Board::from_logical_netlist` rejects an unsupported schema, duplicate identities, incomplete pin/net membership, and any component without an exact footprint/pad map.

Coordinates use signed integer micrometres through `jingwei_geometry::Length`; angles use signed integer microdegrees. `GEOMETRY_CONVENTION_VERSION` identifies the convention. Physical geometry does not use untyped floating-point values.

## Produced domain APIs

- Board/model: `BoardDefinition`, `Stackup`, `Footprint`, `Pad`, `NetClass`, `Track`, `Via`, `Zone`, `Keepout`, regions, holes, text, provenance, and edit controls.
- Placement: `place_baseline(&Board, &PlacementRequest) -> PlacementCandidate`.
- Routing: async `RouterBackend::route`, `ReferenceRouter`, `FakeRouter`, `route_critical`, `probe_freerouting`, and deterministic DSN export.
- Validation: `run_native_drc(&Board) -> DrcReport`, bound to candidate revision, board hash, profile, rule version, object IDs, geometry, severity, evidence, and suggested fix.
- PCB view/edit: `ViewRequest`, `DisplayList`, hit-test/snap, `CrossProbeResult`, `SemanticEditCommand`, preview, and isolated `EditCandidate` with invalidated validation scopes.
- Projection: `project_board`, bounded typed S-expression parse/serialize, `check_equivalence`, object map/coverage, and `run_reference_drc`.
- Manufacturing: bounded profile load/validate, `export_candidate`, `verify_candidate`, immutable candidate directory, SHA-256 manifest, and `release_readiness`.

## Task 02 registration

`register_physical_module()` uses the task-02-owned `jingwei_jdp::ToolDescriptor` directly. It registers every task-book tool, including separate `pcb_render` and `pcb_project`, and deliberately omits Production Release operations.

The current task-02 contract pack has descriptors and envelopes but no executable `DomainToolModule`, `ValidationProvider`, `ViewProvider`, or `ProjectionBackend` trait/registry. Therefore this branch supplies implementation APIs and descriptors, not a daemon binding. The kernel remains the sole owner of revision transactions and active-pointer movement. The proposed adapter is documented in `docs/interface-change-proposals/task-04-physical-module-registry.md`.

## Task 05 adaptation

The workbench should treat preview as non-mutating ghost geometry, render the returned stable entity IDs, show affected nets and local validation, and send committed semantic commands to the kernel. Locked/edit-controlled entities remain blocking, and an edit candidate never implies project-pointer movement.
