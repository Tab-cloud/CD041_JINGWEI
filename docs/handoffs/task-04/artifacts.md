# Task 04 artifacts

## Golden manufacturing candidate

Candidate directory:

```text
testdata/physical/candidates/pcb-candidate-logical-golden-rev-001-placement-baseline-grid-cluster-v1-000000004a573034-route-manhattan-reference-router-v1-000000004a573034--golden
```

Design revision:

```text
pcb-candidate:logical-golden-rev-001:placement:baseline-grid-cluster-v1:000000004a573034:route:manhattan-reference-router-v1:000000004a573034
```

`manifest.json` SHA-256:

```text
6a7b7aed5568387b1ee27d9aa0097a044cfdd82493197862e018fc57fe9d0fd1
```

The same digest is stored in `manifest.sha256`.

## Reports

- `reports/native-drc.json`: executed native rule report; zero errors with explicit coverage items.
- `reports/reference-gates.json`: KiCad/Freerouting external reference status; skipped/unavailable, not passed.
- `projection/object-map.json`: stable JIR entity to KiCad UUID mapping.
- `projection/coverage.json`: represented and unsupported feature coverage.
- `testdata/physical/reports/golden-package-verification.json`: independent readback report; `validManufacturingCandidate` is true and findings are empty.

## Fabrication/assembly content

- Gerber copper, mask, silkscreen, and unique closed board outline, plus Gerber Job.
- Combined PTH/NPTH Excellon drill and IPC-356 connectivity candidate.
- BOM, CPL with explicit millimetre/origin/rotation convention, and assembly SVG.
- Native board-envelope STEP candidate.
- KiCad-compatible typed S-expression projection.
- Source board/revision snapshots and a version/hash/profile-bound manifest.

Every artifact belongs to the same design revision and is stored below a new immutable candidate directory. The package is not a ZIP and is not a factory order.
