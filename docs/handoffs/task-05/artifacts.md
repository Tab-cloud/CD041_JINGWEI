# Task 05 artifacts

## Build artifacts

Build outputs are intentionally ignored and are not committed.

```text
app/src-tauri/target/release/bundle/nsis/Jingwei Agent Dev_0.1.0_x64-setup.exe
size: 2,698,659 bytes
SHA-256: AB616454BA85308D63EE94C79B42E444E244937C612033E430426DBFF852E2C4
signature: NotSigned

app/src-tauri/target/release/jingwei-workbench.exe
size: 11,867,136 bytes
SHA-256: A1B574782AC9825B5F25885AA7F80896F27CCA8E5FC2F4A9F464C82434E35B91

app/src-tauri/target/release/sidecars/manifest.json
size: 500 bytes
SHA-256: 1DD0C5218BB5D157CA72F13E420605AE144968BD8FDD30E3F9A78A1A86CA611E
```

The installer and executable are development-only. They are not signed and contain no real runtime/daemon sidecars.

## Desktop evidence

Committed screenshots are under `docs/handoffs/task-05/evidence/`:

- `tauri-final-package-smoke.jpg` — final release executable, prominent mock boundary, four-region native workbench, sidecar unavailable state, and Production Release blockers. SHA-256 `9A7162400E85F807C6A52548D88A69F8861CE2442D6E7CE1876CDE14345A19C3`.
- `tauri-final-edit-matrix.jpg` — final release executable after wire, symbol move, footprint move, via, and track commits; draft at `rev-draft-016`, current unchanged. SHA-256 `DD6742A7BC7BDEE3403B2A5CC6519BCDF31C1C75806C42A9D4940542E71BC6F1`.
- `tauri-recovery-after-refresh.jpg` — reload recovery with caught-up replay cursor and recovered server-backed draft. SHA-256 `A4931F48EBFBEF82C919CF008BD02E32CD79ED1BEE7F2DD6698111C0BB81A3F8`.
- `tauri-pcb-workbench.jpg` — PCB semantic editor, AI patch review, selection/provenance, and release-gap presentation. SHA-256 `8E2F4F6D29D79576CE5E2C5FC0A932DDC2821DB588DC2B8F969F1A95A24FBCB5`.

## Fixture provenance

- `app/src/mocks/v0.1/fixtures.ts` contains UI-consumable versioned mock state.
- `testdata/ui/v0.1/scenario-manifest.json` identifies the mock scenarios and states that they are not production evidence.
- `testdata/ui/v0.1/benchmark-profile.json` records the synthetic 10k/100k display-list targets.
- `app/src-tauri/sidecars/manifest.json` records both real sidecars as not bundled and prevents a false launch-success claim.
