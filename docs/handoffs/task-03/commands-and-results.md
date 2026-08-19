# Task 03 commands and results

Environment: Windows 11 x64, Rust/Cargo 1.97.1, KiCad CLI 10.0.5.

## Passed

```text
cargo test -p jingwei-components -p jingwei-library -p jingwei-schematic \
  -p jingwei-erc -p jingwei-kicad-schematic
```

Result: passed. 18 test cases passed across unit and documentation targets,
0 failed.

```text
cargo fmt -p jingwei-components -p jingwei-library -p jingwei-schematic \
  -p jingwei-erc -p jingwei-kicad-schematic -- --check
```

Result: passed.

```text
cargo clippy -p jingwei-components -p jingwei-library -p jingwei-schematic \
  -p jingwei-erc -p jingwei-kicad-schematic --all-targets --all-features \
  --no-deps -- -D warnings
```

Result: passed for Task 03 targets. Shared Task 02 crates emitted existing
documentation warnings while compiling as dependencies; `--no-deps` prevented
those external clippy lints from being promoted to Task 03 failures.

## Shared-baseline blocker

The exact task command without `--no-deps` currently fails in `jingwei-jir` on
strict clippy/documentation findings. Task 03 does not own those files and did
not modify them. Final exit code: 101, with 92 errors reported for
`jingwei-jir` before compilation stopped.

The workspace-wide `cargo fmt --all -- --check` also exits 1 on pre-existing
format differences under `jingwei-jir` and a newline-style mismatch in the
Task 04-owned `jingwei-manufacturing` example. The equivalent scoped Task 03
format check passes.

## Fixture and hygiene checks

- All checked-in JSON fixtures/reports parse successfully with PowerShell
  `ConvertFrom-Json`; both native summary files also pass the Rust structural
  S-expression parser.
- Every fixture/review artifact has a SHA-256 digest in `artifacts.md`.
- A read-only scan of every source, documentation and structured-data path in
  `changed-files.txt` found no trailing whitespace. The raw SVG emitted by
  KiCad is retained byte-for-byte and excluded from source-style whitespace
  normalization.
- `git -c core.whitespace=cr-at-eol diff --cached --check` exits 0 when the raw
  KiCad-rendered SVG is excluded. The explicit `cr-at-eol` setting matches the
  repository's Windows CRLF files; the independent path scan covers all Task 03
  source and handoff content.

## KiCad 10.0.5 installation and native-output gate

- Official source tag: `10.0.5`, upstream commit `18fb9289`.
- Source archive SHA-256:
  `661b661199ddf3552362bba85e918267c93d367fddcc2c0e475f1b7b62659d40`.
- Windows x64 installer SHA-256:
  `d2702ed1fabf2b52bd631450407f141563bc14393a607fb335b63194a8284d33`.
- Authenticode: `Valid`; signer `KICAD SERVICES CORPORATION`; certificate
  serial `39DF6B588D969CA15D8D2756`, matching the official KiCad download page.
- Installed through Windows Package Manager as `KiCad.KiCad` 10.0.5 for the
  current user.
- Installed CLI:
  `C:\Users\Administrator\AppData\Local\Programs\KiCad\10.0\bin\kicad-cli.exe`.
- `kicad-cli --version`: `10.0.5`.
- Official upstream minimal schematic: CLI ERC exit 0, 0 violations; JSON report
  identifies KiCad 10.0.5. SVG export exit 0 and produced a 60,442-byte SVG.
- Real GUI: opened the upstream `20210126` schematic and saved it successfully
  as KiCad format version `20260306`.
- GUI-saved file SHA-256:
  `54ed14cd38072a160ba3bf61816cc12ad801cb859b9f92f5deb53c217630d65d`.
- Post-GUI CLI: ERC exit 0, KiCad version 10.0.5, 0 violations; SVG export
  exit 0 and produced a 62,868-byte SVG.

The Task 03 native bundle is generated with:

```text
cargo run -p jingwei-kicad-schematic --example generate_native_fixture
```

Result: native `20220904` schematic, project-local symbol library/table,
projection JSON, object-map JSON and parser summary created under
`testdata/logical/reports/kicad-native`. Structural counts are 2 embedded
library symbols, 2 placed symbols, 4 pins, 4 labels and 0 no-connect markers.

The actual generated Task 03 schematic then passed:

```text
kicad-cli sch erc --format json --severity-all --exit-code-violations \
  --output jingwei-native-reference.erc.json \
  jingwei-native-reference.kicad_sch
kicad-cli sch export svg --output rendered \
  jingwei-native-reference.kicad_sch
```

Result: ERC exit 0, KiCad version 10.0.5, 0 violations; SVG export exit 0
and produced a 101,229-byte SVG.

The real KiCad 10.0.5 schematic editor opened the generated Task 03 file,
showed both symbols and all four pin/net labels, and saved the upgraded file as
`jingwei-native-reference-gui-saved.kicad_sch`. The title changed to the saved
filename and both editor windows were closed after verification. The saved
file reports format `20260306`; the Task 03 parser reads it back with unchanged
counts (2 symbols, 4 pins and 4 labels). A second CLI ERC run on this GUI-saved
file exited 0 with KiCad 10.0.5 and 0 violations.

The installer, extracted GPLv3 source and runtime evidence are kept outside the
repository under `D:\Projects\CD041_LUBAN\.tooling\kicad\10.0.5`.

This closes the installation, real GUI/CLI reference gate and Task 03 native
projection gate. GUI session history, locks and preference files are archived
outside the repository; only reproducible inputs and acceptance evidence remain
under `testdata/logical/reports/kicad-native`.
