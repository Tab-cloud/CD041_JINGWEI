//! Atomic in-process resource claim arbitration for concurrent daemon requests.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

/// Read or write access to a named resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimMode {
    Read,
    Write,
}

/// One explicit resource claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceClaim {
    pub resource: String,
    pub mode: ClaimMode,
}

#[derive(Clone, Debug)]
struct HeldClaim {
    owner: String,
    mode: ClaimMode,
}

/// Duplicate-safe resource claim manager shared by all transactions in one kernel.
#[derive(Clone, Default)]
pub struct ResourceClaimManager {
    held: Arc<Mutex<BTreeMap<String, Vec<HeldClaim>>>>,
}

impl ResourceClaimManager {
    /// Acquires all claims atomically or none of them.
    pub fn acquire(
        &self,
        owner: impl Into<String>,
        mut claims: Vec<ResourceClaim>,
    ) -> Result<ClaimGuard, ResourceConflict> {
        let owner = owner.into();
        claims.sort_by(|left, right| left.resource.cmp(&right.resource));
        claims.dedup();
        if claims.iter().any(|claim| claim.resource.is_empty()) {
            return Err(ResourceConflict {
                resource: String::new(),
            });
        }
        let mut held = self.held.lock().expect("claim registry poisoned");
        for claim in &claims {
            if held.get(&claim.resource).is_some_and(|existing| {
                existing.iter().any(|existing| {
                    existing.owner != owner
                        && (existing.mode == ClaimMode::Write || claim.mode == ClaimMode::Write)
                })
            }) {
                return Err(ResourceConflict {
                    resource: claim.resource.clone(),
                });
            }
        }
        for claim in &claims {
            held.entry(claim.resource.clone())
                .or_default()
                .push(HeldClaim {
                    owner: owner.clone(),
                    mode: claim.mode,
                });
        }
        drop(held);
        Ok(ClaimGuard {
            manager: self.clone(),
            owner,
            claims,
        })
    }
}

/// RAII guard releasing all acquired resources on every return path.
pub struct ClaimGuard {
    manager: ResourceClaimManager,
    owner: String,
    claims: Vec<ResourceClaim>,
}

impl Drop for ClaimGuard {
    fn drop(&mut self) {
        let mut held = self.manager.held.lock().expect("claim registry poisoned");
        for claim in &self.claims {
            if let Some(existing) = held.get_mut(&claim.resource) {
                existing.retain(|entry| entry.owner != self.owner);
                if existing.is_empty() {
                    held.remove(&claim.resource);
                }
            }
        }
    }
}

/// Conflicting claim failure.
#[derive(Clone, Debug, thiserror::Error)]
#[error("resource is already claimed incompatibly: {resource}")]
pub struct ResourceConflict {
    pub resource: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_claims_share_but_write_conflicts() {
        let manager = ResourceClaimManager::default();
        let resource = |mode| {
            vec![ResourceClaim {
                resource: "project:p".into(),
                mode,
            }]
        };
        let first = manager
            .acquire("reader-1", resource(ClaimMode::Read))
            .unwrap();
        let second = manager
            .acquire("reader-2", resource(ClaimMode::Read))
            .unwrap();
        assert!(
            manager
                .acquire("writer", resource(ClaimMode::Write))
                .is_err()
        );
        drop(first);
        drop(second);
        manager
            .acquire("writer", resource(ClaimMode::Write))
            .unwrap();
    }
}
