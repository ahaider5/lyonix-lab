use crate::domain::{HardwareSnapshot, RuntimeProbe, RuntimeStatus};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticReport {
    pub app_version: String,
    pub os: String,
    pub hardware: HardwareSnapshot,
    pub runtimes: Vec<RuntimeProbe>,
    pub runtime_status: RuntimeStatus,
    pub recent_log: Option<PathBuf>,
}

pub fn write_report(path: impl AsRef<Path>, report: &DiagnosticReport) -> Result<(), std::io::Error> {
    let bytes = serde_json::to_vec_pretty(report).map_err(std::io::Error::other)?;
    std::fs::write(path, bytes)
}
