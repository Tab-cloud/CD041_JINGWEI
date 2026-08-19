# Task 04 known gaps

## Open external gates

- KiCad 10.0.5 reference DRC and manufacturing export were skipped because `kicad-cli` is not installed. Projection parsing and structural equivalence tests do not replace this gate.
- Freerouting execution was skipped because no pinned Freerouting binary is installed/configured. The adapter validates an expected binary version/hash and records an explicit skip/rejection; only DSN fixture export was exercised.
- Manufacturer DFM feedback, sample fabrication, assembly, electrical/functional test, and named release approvals are outside task 04 and remain blocking.

## Deliberate Phase 0 limitations

- Placement is a deterministic baseline heuristic, not a commercial optimizer.
- Routing has no differential-pair geometry, length matching, controlled impedance, push-and-shove, RF, SI, PI, or return-path solver.
- Native DRC includes useful assembly/test-point heuristics but does not establish full thermal, EMC, SI, PI, reliability, creepage/clearance safety-standard, or factory-process compliance.
- Zone fill currently establishes a validated candidate state but is not a production polygon-boolean/thermal-spoke engine.
- The STEP output is a bounded native board-envelope candidate, not a component-populated mechanical interchange proven by an external CAD parser.
- The checked-in manufacturer profile is deliberately conservative and limited to the two-layer golden case. Real ordering requires current factory confirmation.

## Integration gaps

- Task 03's live public logical-netlist adapter is not yet wired; the versioned fixture is used instead.
- Task 02 has no executable domain-module/provider registry yet, so descriptors are registered locally and must be bound without duplicating the formal descriptor types.
- Task 05 must integrate display/edit APIs and route every commit through the task-02 kernel transaction service.
- The task-02 generic view contract currently uses bare `f64` coordinates. PCB integration should preserve the task-04 integer-micrometre convention or add explicit units at the boundary.

## Workspace state

The repository was concurrently initialized on `feat/jw-05-workbench` at base commit `a238f49`; task 04 did not switch branches, stage, commit, or alter files owned by other sessions. The task-04 file set is isolated under the paths listed in `changed-files.txt`.
