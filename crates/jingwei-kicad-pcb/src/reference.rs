use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Pinned `KiCad` reference backend configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KicadReferenceConfig {
    /// Absolute `kicad-cli` path.
    pub executable: PathBuf,
    /// Exact expected version text.
    pub expected_version: String,
    /// Expected executable SHA-256.
    pub expected_sha256: String,
    /// Maximum captured stdout/stderr bytes.
    pub maximum_output_bytes: usize,
}

/// Reference backend terminal state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReferenceRunStatus {
    /// `KiCad` executed and the report was produced.
    Executed,
    /// Tool was unavailable; gate remains open.
    Skipped,
    /// Tool pin, process or report validation failed.
    Failed,
}

/// Structured `KiCad` reference DRC execution evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceDrcReport {
    /// Terminal state.
    pub status: ReferenceRunStatus,
    /// Observed version when available.
    pub version: Option<String>,
    /// Verified executable hash when available.
    pub executable_sha256: Option<String>,
    /// Exact argument vector without shell interpretation.
    pub arguments: Vec<String>,
    /// Exit code.
    pub exit_code: Option<i32>,
    /// Bounded stdout.
    pub stdout: String,
    /// Bounded stderr.
    pub stderr: String,
    /// Report path.
    pub report_path: Option<PathBuf>,
    /// Explicit skip/failure reason.
    pub reason: Option<String>,
}

/// Runs `kicad-cli pcb drc` without GUI automation after version/hash verification.
#[allow(clippy::too_many_lines)]
pub fn run_reference_drc(
    config: &KicadReferenceConfig,
    board_path: &Path,
    report_path: &Path,
) -> ReferenceDrcReport {
    let base = || ReferenceDrcReport {
        status: ReferenceRunStatus::Failed,
        version: None,
        executable_sha256: None,
        arguments: vec![],
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        report_path: None,
        reason: None,
    };
    if !config.executable.is_file() {
        return ReferenceDrcReport {
            status: ReferenceRunStatus::Skipped,
            reason: Some(format!(
                "kicad-cli not found at {}",
                config.executable.display()
            )),
            ..base()
        };
    }
    if !board_path.is_file() {
        return ReferenceDrcReport {
            reason: Some(format!("projection not found: {}", board_path.display())),
            ..base()
        };
    }
    let hash = match hash_file(&config.executable) {
        Ok(value) => value,
        Err(error) => {
            return ReferenceDrcReport {
                reason: Some(format!("cannot hash kicad-cli: {error}")),
                ..base()
            };
        }
    };
    if !hash.eq_ignore_ascii_case(&config.expected_sha256) {
        return ReferenceDrcReport {
            executable_sha256: Some(hash),
            reason: Some("kicad-cli hash does not match toolchain lock".to_owned()),
            ..base()
        };
    }
    let version_output = match Command::new(&config.executable).arg("version").output() {
        Ok(output) => output,
        Err(error) => {
            return ReferenceDrcReport {
                executable_sha256: Some(hash),
                reason: Some(format!("version probe failed: {error}")),
                ..base()
            };
        }
    };
    let version = bounded_text(&version_output.stdout, config.maximum_output_bytes);
    if !version_output.status.success() || version.trim() != config.expected_version {
        return ReferenceDrcReport {
            version: Some(version),
            executable_sha256: Some(hash),
            exit_code: version_output.status.code(),
            reason: Some("kicad-cli version does not match toolchain lock".to_owned()),
            ..base()
        };
    }
    if let Some(parent) = report_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        return ReferenceDrcReport {
            version: Some(version.trim().to_owned()),
            executable_sha256: Some(hash),
            reason: Some(format!("cannot create report directory: {error}")),
            ..base()
        };
    }
    let arguments = vec![
        "pcb".to_owned(),
        "drc".to_owned(),
        "--output".to_owned(),
        report_path.display().to_string(),
        board_path.display().to_string(),
    ];
    let output = match Command::new(&config.executable).args(&arguments).output() {
        Ok(output) => output,
        Err(error) => {
            return ReferenceDrcReport {
                version: Some(version.trim().to_owned()),
                executable_sha256: Some(hash),
                arguments,
                reason: Some(format!("reference DRC process failed: {error}")),
                ..base()
            };
        }
    };
    let report_exists = report_path.is_file();
    ReferenceDrcReport {
        status: if output.status.success() && report_exists {
            ReferenceRunStatus::Executed
        } else {
            ReferenceRunStatus::Failed
        },
        version: Some(version.trim().to_owned()),
        executable_sha256: Some(hash),
        arguments,
        exit_code: output.status.code(),
        stdout: bounded_text(&output.stdout, config.maximum_output_bytes),
        stderr: bounded_text(&output.stderr, config.maximum_output_bytes),
        report_path: report_exists.then(|| report_path.to_path_buf()),
        reason: (!output.status.success() || !report_exists)
            .then(|| "kicad-cli failed or did not produce the requested report".to_owned()),
    }
}

fn bounded_text(bytes: &[u8], maximum: usize) -> String {
    let end = bytes.len().min(maximum);
    String::from_utf8_lossy(&bytes[..end]).into_owned()
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
    fn missing_kicad_is_an_explicit_skip() {
        let report = run_reference_drc(
            &KicadReferenceConfig {
                executable: PathBuf::from("definitely-not-installed-kicad-cli"),
                expected_version: "10.0.5".to_owned(),
                expected_sha256: "00".repeat(32),
                maximum_output_bytes: 1024,
            },
            Path::new("board.kicad_pcb"),
            Path::new("drc.rpt"),
        );
        assert_eq!(report.status, ReferenceRunStatus::Skipped);
        assert!(report.reason.unwrap().contains("not found"));
    }
}
