# UI testdata

`v0.1/` contains transport-independent metadata for the task-05 workbench:

- `scenario-manifest.json` names normal, failure, disconnect, sidecar, and maturity scenarios expected from the versioned mock clients.
- `benchmark-profile.json` records the 10k/100k display-list acceptance profiles and budgets.

Executable fixture objects remain beside the mock clients in `app/src/mocks/v0.1/fixtures.ts` so TypeScript tests validate them against the UI facade types. This directory holds only stable scenario metadata, not screenshots or generated build output.
