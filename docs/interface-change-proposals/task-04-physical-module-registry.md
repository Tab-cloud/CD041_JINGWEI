# Interface change proposal: executable physical-domain providers

Owner requesting change: task 04. Proposed owner: task 02 contract/kernel, with task 05 consuming the view adapter.

## Problem

Task 02 currently owns the correct formal `ToolDescriptor`, permissions, effects, envelopes, validation values, and generic view data. It does not yet expose an executable domain-module registry or provider traits for validation, projection, PCB display/hit-test, and semantic edits. Task 04 can therefore publish correct descriptors and library APIs, but cannot bind handlers without inventing a second formal registry type or modifying task-02-owned files.

The generic task-02 view `Point`, `Rect`, hit-test tolerance, and distance use bare `f64`. That conflicts with the task-00/task-04 requirement that physical coordinates have explicit units and deterministic quantization.

## Proposed contract-pack additions

Add kernel-owned adapter traits whose methods receive the authenticated JDP execution context, base revision, cancellation token, resource claims, and bounded artifact workspace. The exact async abstraction may follow task 02's runtime choice, but it should cover:

```rust
trait DomainToolModule {
    fn module_id(&self) -> &'static str;
    fn descriptors(&self) -> Vec<ToolDescriptor>;
    async fn execute(&self, request: JdpRequest<Value>, context: ToolContext)
        -> JdpResponse<Value>;
}

trait ValidationProvider { /* revision-bound native/reference reports */ }
trait ViewProvider { /* display, hit-test, snap, cross-probe */ }
trait ProjectionBackend { /* projection, object map, coverage, reference checks */ }
```

The kernel adapter, not a domain module, must create/commit revisions, move pointers, enforce resource claims, and journal operations. A physical handler should return an isolated candidate/diff, artifact references, validation scopes, and findings.

## Unit-safe view boundary

For PCB views, either:

1. change generic geometry to a serializable quantity `{ value, unit }`; or
2. define a versioned integer-micrometre physical-view adapter and convert to screen pixels only in task 05.

The second option is the smaller Phase 0 change and preserves exact entity geometry across display, hit-test, edit preview, and DRC locations.

## Descriptor schemas

Task 04 currently uses permissive object schemas because task 02 has no code-generated per-handler schema hook. Add a schema registration path so the physical module can publish bounded JSON Schema derived from its input/output DTOs. Reject unknown or oversized fields at the daemon boundary before invoking exporters/parsers.

## Proposed integration

- Task 02 adds the traits/registry and a transaction-owned execution context.
- Task 04 supplies an adapter over `register_physical_module()`, `Board`, placement/routing, DRC, projection, export, and verification APIs.
- Task 03 supplies `logical-netlist-v0` through a public adapter; task 04 continues exact-footprint/pad-map validation.
- Task 05 converts integer micrometres to viewport pixels and submits semantic edit candidates through the task-02 transaction path.

No Production Release handler should be added to the physical module. `release_readiness` remains read-only and reports the missing external gates.
