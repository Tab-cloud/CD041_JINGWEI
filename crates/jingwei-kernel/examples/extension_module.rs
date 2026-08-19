//! Minimal example showing how sessions 03/04 compose a validator without store or IPC access.

use async_trait::async_trait;
use jingwei_jdp::{ValidationFinding, ValidationOutput};
use jingwei_kernel::{ExtensionRegistryBuilder, ValidationContext, ValidationProvider};

struct ExampleValidator;

#[async_trait]
impl ValidationProvider for ExampleValidator {
    fn name(&self) -> &'static str {
        "example.non-empty-design"
    }

    async fn validate(
        &self,
        graph: &jingwei_jir::JirGraph,
        _context: &ValidationContext,
    ) -> ValidationOutput {
        let valid = !graph.entities.is_empty();
        ValidationOutput {
            valid,
            findings: (!valid)
                .then(|| ValidationFinding {
                    code: "EXAMPLE_EMPTY_DESIGN".into(),
                    severity: "error".into(),
                    summary: "design must contain at least one entity".into(),
                    object_refs: Vec::new(),
                    fingerprint: "example-empty-design".into(),
                })
                .into_iter()
                .collect(),
            providers_run: vec![self.name().into()],
        }
    }
}

fn main() {
    let registry = ExtensionRegistryBuilder::new()
        .validator(ExampleValidator)
        .expect("unique validator")
        .build();
    assert_eq!(registry.validator_names(), vec!["example.non-empty-design"]);
}
