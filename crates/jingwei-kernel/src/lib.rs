//! Authoritative design transaction service and explicit extension composition boundary.

mod claims;
mod draft;
mod extensions;
mod kernel;
mod patch;
mod task;

pub use claims::{ClaimGuard, ClaimMode, ResourceClaim, ResourceClaimManager};

pub use extensions::{
    DependencyInvalidationRule, DomainToolModule, ExtensionRegistry, ExtensionRegistryBuilder,
    ProjectionBackend, ProjectionRequest, ProjectionResult, SemanticEditHandler, ToolContext,
    ToolRequest, ToolResult, ValidationContext, ValidationProvider, ViewProvider,
};
pub use kernel::{CompileCommit, Kernel, KernelError, RequestContext};
pub use patch::{AppliedPatch, apply_patch_to_graph, evaluate_preconditions};
pub use task::{TaskMachine, TaskState, TaskTransitionError};
