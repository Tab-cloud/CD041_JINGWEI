use jingwei_pcb::Board;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Pinned optional Freerouting adapter configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeroutingConfig {
    /// Absolute path to a reviewed binary/JAR wrapper.
    pub executable: PathBuf,
    /// Exact expected version text.
    pub expected_version: String,
    /// SHA-256 of the executable.
    pub expected_sha256: String,
    /// Fixed arguments approved by policy.
    pub fixed_arguments: Vec<String>,
    /// Maximum output size.
    pub maximum_output_bytes: u64,
}

/// Structured availability report; absence never masquerades as a passed route gate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum FreeroutingAvailability {
    /// Pinned binary and version were verified.
    Available {
        /// Observed version text.
        version: String,
        /// Verified executable hash.
        sha256: String,
    },
    /// Binary is not installed.
    Skipped {
        /// Human-readable reason.
        reason: String,
    },
    /// Binary exists but violates the pin/policy.
    Rejected {
        /// Human-readable reason.
        reason: String,
    },
}

/// Verifies the optional executable without routing a design.
pub fn probe_freerouting(config: &FreeroutingConfig) -> FreeroutingAvailability {
    if !config.executable.is_file() {
        return FreeroutingAvailability::Skipped {
            reason: format!("binary not found: {}", config.executable.display()),
        };
    }
    let observed_hash = match hash_file(&config.executable) {
        Ok(hash) => hash,
        Err(error) => {
            return FreeroutingAvailability::Rejected {
                reason: format!("cannot hash binary: {error}"),
            };
        }
    };
    if !observed_hash.eq_ignore_ascii_case(&config.expected_sha256) {
        return FreeroutingAvailability::Rejected {
            reason: format!(
                "binary hash mismatch: expected {}, observed {observed_hash}",
                config.expected_sha256
            ),
        };
    }
    let output = match Command::new(&config.executable).arg("--version").output() {
        Ok(output) => output,
        Err(error) => {
            return FreeroutingAvailability::Rejected {
                reason: format!("version probe failed: {error}"),
            };
        }
    };
    if !output.status.success() {
        return FreeroutingAvailability::Rejected {
            reason: format!("version probe exited with {}", output.status),
        };
    }
    let mut version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if version.is_empty() {
        String::from_utf8_lossy(&output.stderr)
            .trim()
            .clone_into(&mut version);
    }
    if version != config.expected_version {
        return FreeroutingAvailability::Rejected {
            reason: format!(
                "version mismatch: expected {}, observed {version}",
                config.expected_version
            ),
        };
    }
    FreeroutingAvailability::Available {
        version,
        sha256: observed_hash,
    }
}

/// Exports a bounded, deterministic Specctra DSN-like fixture for an isolated adapter directory.
///
/// This is an adapter interchange fixture, not a claim of complete Specctra coverage.
pub fn export_specctra_dsn(board: &Board) -> String {
    let bounds = board.definition.outline.bounds();
    let mut text = String::from("(pcb jingwei\n  (parser string_quote \" )\n");
    let _ = writeln!(
        text,
        "  (structure (boundary (rect pcb {} {} {} {})))",
        bounds.min.x.as_micrometres(),
        bounds.min.y.as_micrometres(),
        bounds.max.x.as_micrometres(),
        bounds.max.y.as_micrometres()
    );
    text.push_str("  (network\n");
    for (id, name) in &board.nets {
        let _ = writeln!(text, "    (net {id} (name {name}))");
    }
    text.push_str("  )\n)\n");
    text
}

fn hash_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(hex::encode(digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_binary_is_an_explicit_skip() {
        let report = probe_freerouting(&FreeroutingConfig {
            executable: PathBuf::from("definitely-not-installed-freerouting"),
            expected_version: "2.2.4".to_owned(),
            expected_sha256: "00".repeat(32),
            fixed_arguments: vec![],
            maximum_output_bytes: 1024,
        });
        assert!(matches!(report, FreeroutingAvailability::Skipped { .. }));
    }
}
