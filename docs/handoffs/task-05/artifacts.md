# Task 05 artifacts

## Integration revalidation (current)

Task 05's original mock-only artifact record remains below as historical handoff evidence. The integration branch now produces
and verifies a larger installer with real Windows runtime and daemon sidecars:

```text
app/src-tauri/target/release/bundle/nsis/Jingwei Agent Dev_0.1.0_x64-setup.exe
size: 30,461,443 bytes
SHA-256: 6B5444C3A6FC2E4CADCB85390B3B6BB77B3F2914ECB6C974A9807FA067D9AC60
signature: NotSigned

installed jingwei-workbench.exe
size: 11,902,976 bytes
SHA-256: 81615129DFF4C8D416647A5E2845358FFE94C1BE35563A18794404ABA1918AAA

installed jingwei-agent-runtime-x86_64-pc-windows-msvc.exe
size: 113,857,536 bytes
SHA-256: 5FDA5F7162CD83BC1ECBE48D5E6457B574FD77C08F0968FE133A1B689D92167F

installed jingwei-daemon-x86_64-pc-windows-msvc.exe
size: 3,932,672 bytes
SHA-256: CF580837946D6331E966C6545C5777CE3DC2A826787209B1E8E8BCDFDF9136C2
```

The installed app reached healthy for both sidecars after one recheck, used binaries from the installed resource directory,
switched dark to light theme, displayed the model/API configuration drawer and missing environment-variable reasons, preserved
the mock-data and Production Release boundaries, and left no Jingwei processes after exit. Machine-readable evidence is in
`testdata/sidecars/windows-installed-app-gate.json` and `testdata/sidecars/windows-sidecar-gate.json`.

## Build artifacts

Build outputs are intentionally ignored and are not committed.

```text
app/src-tauri/target/release/bundle/nsis/Jingwei Agent Dev_0.1.0_x64-setup.exe
size: 2,713,792 bytes
SHA-256: 5A22EBFC9990C4FD70A20EA1E0EA91D873C973959DE55E103031C3B335D6CF6E
signature: NotSigned

app/src-tauri/target/release/jingwei-workbench.exe
size: 11,879,424 bytes
SHA-256: 18FD6C55B9DB248DEBD9FA2046CFADF3FB0958DA7AA81DB35C52C93B4B595A38

app/src-tauri/target/release/sidecars/manifest.json
size: 499 bytes
SHA-256: E88E29F5D2E6DE385653174E28EE0095F503A796ACF43417496E2AFF5E3ACBA9
```

These original Task 05 installer hashes are development-only historical artifacts. They were unsigned and contained no real
runtime/daemon sidecars; use the integration revalidation section above for the current package.

The release executable was launched after the model-settings change. The native drawer exposed provider, HTTPS endpoint, model ID, password-style API Key input, environment/local status cards, and `SKIPPED · 未执行`. No configuration or credential was saved during this visual gate.

## Desktop evidence

Committed screenshots are under `docs/handoffs/task-05/evidence/`:

The four interaction screenshots below predate the branding correction and remain evidence of workbench behavior, not of the
current 精卫 icon/title. The current rebuilt executable icon is verified separately by the extracted-icon evidence.

- `tauri-final-package-smoke.jpg` — final release executable, prominent mock boundary, four-region native workbench, sidecar unavailable state, and Production Release blockers. SHA-256 `9A7162400E85F807C6A52548D88A69F8861CE2442D6E7CE1876CDE14345A19C3`.
- `tauri-final-edit-matrix.jpg` — final release executable after wire, symbol move, footprint move, via, and track commits; draft at `rev-draft-016`, current unchanged. SHA-256 `DD6742A7BC7BDEE3403B2A5CC6519BCDF31C1C75806C42A9D4940542E71BC6F1`.
- `tauri-recovery-after-refresh.jpg` — reload recovery with caught-up replay cursor and recovered server-backed draft. SHA-256 `A4931F48EBFBEF82C919CF008BD02E32CD79ED1BEE7F2DD6698111C0BB81A3F8`.
- `tauri-pcb-workbench.jpg` — PCB semantic editor, AI patch review, selection/provenance, and release-gap presentation. SHA-256 `8E2F4F6D29D79576CE5E2C5FC0A932DDC2821DB588DC2B8F969F1A95A24FBCB5`.
- `jingwei-logo-exe-icon.png` — icon extracted from the rebuilt Windows executable after the product-name correction to 精卫 Agent. SHA-256 `808C88A94D7C100FC7569C0F7E9C4F35F9CC6D0566521E0EF50522E2C2EA11FC`.

## Brand icon sources

- `app/src-tauri/icons/app-icon.svg` is the production vector master: 精卫 carries an amber stone over two sea waves; three circuit traces in the wing connect the myth to AI-native EDA.
- `app/src-tauri/icons/jingwei-logo-source.png` preserves the built-in image-generation concept source.
- `app/src-tauri/icons/jingwei-logo-vector-preview.png` is the 512 × 512 review rendering used to validate the simplified vector silhouette.
- Tauri-generated Windows/macOS/Linux/iOS/Android icon files under `app/src-tauri/icons/` were regenerated from `app-icon.svg`.

## Fixture provenance

- `app/src/mocks/v0.1/fixtures.ts` contains UI-consumable versioned mock state.
- `testdata/ui/v0.1/scenario-manifest.json` identifies the mock scenarios and states that they are not production evidence.
- `testdata/ui/v0.1/benchmark-profile.json` records the synthetic 10k/100k display-list targets.
- `app/src-tauri/sidecars/manifest.json` now records pinned hashes for both bundled Windows sidecars. The UI still labels mock
  design data separately from real process health.
