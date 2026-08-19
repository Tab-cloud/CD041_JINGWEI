# Task 03 interface proposal: logical domain registry completion

Status: proposed for the Task 02 integration owner
Consumer: Task 03 logical domain
Current shared protocol: JDP `0.1`, JIR `0.1`

## Problem

The current contract pack provides canonical IDs, JIR graph types, tool
descriptors, validation summaries, display primitives, hit testing and an
extensible semantic-edit envelope. It does not yet expose the registry traits
described in the task contracts, so a domain crate cannot register executable
handlers, validation providers, view providers or projection backends without
modifying the daemon composition root.

`jingwei-kicad-schematic::register_logical_module()` therefore returns a typed
registration bundle that reuses current JDP/JIR types but remains a candidate
shim. It does not write revisions or implement another daemon.

## Requested core surface

Add a kernel-owned registration API semantically equivalent to:

```rust
pub trait DomainToolModule: Send + Sync {
    fn descriptors(&self) -> Vec<jingwei_jdp::ToolDescriptor>;
    fn entity_schemas(&self) -> Vec<jingwei_jir::SchemaDescriptor>;
    fn edge_schemas(&self) -> Vec<jingwei_jir::SchemaDescriptor>;
    fn execute(
        &self,
        context: ToolContext,
        request: DomainToolRequest,
    ) -> BoxFuture<'static, DomainToolResult>;
}

pub trait ValidationProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn validate(
        &self,
        revision: RevisionId,
        graph: &JirGraph,
        scope: ValidationScope,
    ) -> BoxFuture<'static, ValidationOutput>;
}

pub trait ViewProvider: Send + Sync {
    fn view(&self, graph: &JirGraph, request: ViewRequest) -> Result<DisplayList, DomainError>;
    fn hit_test(
        &self,
        graph: &JirGraph,
        request: HitTestRequest,
    ) -> Result<HitTestResult, DomainError>;
}

pub trait ProjectionBackend: Send + Sync {
    fn project(
        &self,
        graph: &JirGraph,
        request: ProjectionRequest,
    ) -> BoxFuture<'static, ProjectionResult>;
}
```

The exact async mechanism may remain a Task 02 decision. Registration should be
an explicit builder/layer at the daemon composition root, not runtime loading of
arbitrary dynamic libraries.

## Candidate-operation invariant

Domain handlers must return one of:

- a read-only domain result;
- a candidate set;
- normalized `PatchOperation` or `SemanticEditCommand` values with preconditions,
  postconditions, affected entities and validation scope;
- findings, artifacts or evidence references.

Only the kernel transaction service may create a revision or move the current or
draft pointer. A domain handler must not receive a store handle.

## Schema and finding details

Please publicly re-export `SchemaDescriptor` from `jingwei-jir`; it currently
exists but is not in the crate's public re-export list.

The current `ValidationFinding` summary is adequate for transport fallback but
Task 03 needs a detailed artifact shape containing:

- rule ID and version;
- severity;
- stable object and evidence references;
- message and suggested actions;
- coverage status, checked aspects and gaps;
- deterministic fingerprint.

The existing short `ValidationFinding` can remain in the JDP success envelope if
the detailed list is stored as a referenced validation artifact.

## Cross-probe extension

The common `CrossProbeResult` covers entity-to-entity links. Logical cross-probe
also returns part-decision, footprint-asset, BOM-line and finding references.
Prefer an extensible typed relation payload or a referenced cross-probe artifact;
do not convert those stable references into screen coordinates.

## Compatibility

This proposal does not change wire ID formats, JDP envelopes or JIR ownership.
The current Task 03 shim can be adapted by replacing its bundle with concrete
trait implementations while preserving all domain types and tests.
