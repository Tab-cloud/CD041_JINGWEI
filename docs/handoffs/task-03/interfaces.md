# Task 03 interfaces

## Consumed core interfaces

- `jingwei_jir::EntityId`, `RevisionId`, `EditControl`, `EditPolicy`.
- `jingwei_jir::JirGraph`, `JirEntity`, `JirEdge`, `CanonicalValue`.
- `jingwei_jdp::ToolDescriptor`, permission/effect declarations.
- JDP display, hit-test, cross-probe, semantic edit and validation-summary types.

Protocol/schema baseline: JDP `0.1`, JIR `0.1`, logical domain `0.1`.

## Provided domain APIs

### Components

- `DesignIntent::compile_candidate`
- `classify_risk`
- `PartSourceAdapter` and `LocalCatalogAdapter`
- `compare_candidates`, `select_part`, `validate_substitution`

### Library

- `extract_controlled_datasheet`
- `generate_assets`
- `resolve_library`
- `verify_library`

### Schematic

- `SchematicGraph::validate`, `SchematicGraph::to_jir_graph`
- `preview_commands`, `apply_candidate`
- `display_list`, `hit_test`, `cross_probe`

### ERC and projection

- `run_native_erc`, `rule_manifest`, `ErcRun::to_jdp_validation`
- `project_schematic`, `serialize_projection`, `parse_projection`
- `build_native_artifact`, `serialize_native_schematic`,
  `parse_native_schematic`, `serialize_symbol_library`,
  `serialize_symbol_library_table`, `serialize_object_map`
- `register_logical_module`

All mutating logical APIs return ghost graphs or candidate operations. None owns
a workspace/store handle or moves a revision pointer.

## Session integration points

- **Task 02 registry/kernel:** replace the typed registration shim with the
  proposed executable provider traits. The kernel remains the only revision and
  transaction owner.
- **Task 04 logical netlist/PCB:** consume stable symbol, pin and net entity IDs
  from `SchematicGraph::to_jir_graph`; keep schematic-to-footprint association
  semantic and let the PCB domain own physical placement/routing.
- **Task 05 view/edit:** consume `display_list`, `hit_test`, `cross_probe` and
  `preview_commands`. UI edits must submit the returned semantic command group
  to the kernel instead of mutating graph or projection files directly.

## Proposed core change

See `docs/interface-change-proposals/task-03-logical-domain-registry.md` for the
executable module/validator/view/projection registry surface and detailed finding
artifact requirements.
