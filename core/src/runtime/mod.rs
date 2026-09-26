pub mod inference;
pub mod llama_cpp;
pub mod registry;
pub mod supervisor;

use crate::domain::RuntimeDevice;
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeProbeError {
    #[error("runtime executable not found: {0}")]
    Missing(String),
    #[error("runtime probe failed: {0}")]
    Failed(String),
}

pub fn resolve_executable(configured: Option<&std::path::Path>, command: &str) -> Result<std::path::PathBuf, RuntimeProbeError> {
    if let Some(path) = configured {
        if path.is_file() { return Ok(path.to_path_buf()); }
        return Err(RuntimeProbeError::Missing(path.display().to_string()));
    }
    which::which(command).map_err(|_| RuntimeProbeError::Missing(command.into()))
}

pub fn probe_version(executable: &std::path::Path) -> Result<String, RuntimeProbeError> {
    let output = Command::new(executable).arg("--version").output().map_err(|e| RuntimeProbeError::Failed(e.to_string()))?;
    if !output.status.success() { return Err(RuntimeProbeError::Failed(String::from_utf8_lossy(&output.stderr).trim().into())); }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}

pub fn list_devices(executable: &std::path::Path) -> Result<Vec<RuntimeDevice>, RuntimeProbeError> {
    let output = Command::new(executable).arg("--list-devices").output().map_err(|e| RuntimeProbeError::Failed(e.to_string()))?;
    let combined = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    if !output.status.success() && combined.trim().is_empty() { return Err(RuntimeProbeError::Failed("--list-devices returned no output".into())); }
    Ok(parse_device_list(&combined))
}

pub fn parse_device_list(output: &str) -> Vec<RuntimeDevice> {
    output.lines().filter_map(|line| {
        let line = line.trim();
        let (left, right) = line.split_once(": ")?;
        let mut total = None;
        let mut free = None;
        if let Some(open) = right.rfind("(") {
            let detail = &right[open + 1..].trim_end_matches(')');
            let mut parts = detail.split_whitespace();
            total = parts.next().and_then(|v| v.parse::<u64>().ok()).map(|m| m * 1024 * 1024);
            let _ = parts.next();
            free = parts.next().and_then(|v| v.parse::<u64>().ok()).map(|m| m * 1024 * 1024);
        }
        let backend = left.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>();
        Some(RuntimeDevice { runtime_id: "llama.cpp".into(), device_id: left.into(), backend, name: right.split(" (").next().unwrap_or(right).trim().into(), total_memory_bytes: total, available_memory_bytes: free })
    }).collect()
}
