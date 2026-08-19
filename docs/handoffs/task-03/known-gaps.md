# Task 03 known gaps

- No dedicated Task 03 worktree was available. The user authorized a scoped
  delivery commit on the observed Task 05 branch; it contains only the paths in
  the Task 03 handoff manifest and can be cherry-picked into the intended branch.
- The core executable domain registry traits are not public yet. The registration
  function is a typed candidate shim, not a daemon handler installation.
- No current/draft revision was created by Task 03; this is intentional because
  only the kernel transaction service may write revisions.
- The golden schematic fixture is a public semantic manifest; a full daemon-
  materialized JIR golden revision awaits registry integration.
- The KiCad backend emits a deterministic native `.kicad_sch`, embedded symbols,
  a project-local symbol library/table and a JSON object-map sidecar. KiCad
  10.0.5 GUI open/save, CLI parse/ERC and SVG export have been verified on the
  actual Task 03 output; both pre-save and upgraded files have zero ERC
  violations.
- Native output currently materializes only the root sheet. Explicit wire
  geometry, buses and graphical annotations remain JIR-only. The structural
  parser validates the emitted/current KiCad shapes used by the gate, but it is
  not an arbitrary KiCad editor or an unknown-field-preserving round-trip
  rewriter.
- The SVG is a review artifact, not proof of human library approval.
- Physical decoupling distance and test-probe accessibility are delegated to the
  PCB domain. SI, thermal and EMC remain explicit coverage gaps.
- All part/library facts are synthetic. No real manufacturer or supplier facts
  are qualified for production selection.
- The exact recommended clippy command currently fails in shared Task 02 code on
  strict documentation/pedantic findings. Task 03's own targets pass the same
  checks with `--no-deps`.
