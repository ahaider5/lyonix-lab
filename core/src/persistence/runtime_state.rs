use crate::config::paths;
use crate::persistence::StateError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Durable runtime state for short-lived CLI invocations. This is NOT a
/// replacement for ProcessSupervisor ownership: the supervisor's child-handle
/// check remains the authority for stopping an owned process. This file only
/// records what a later invocation needs to reconcile (pid, executable,
/// fingerprint, endpoint, model identity, start time, log paths).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedRuntimeState {
    pub runtime_id: String,
    pub pid: u32,
    pub executable: PathBuf,
    pub fingerprint: String,
    pub endpoint: String,
    pub model_id: String,
    pub artifact_id: String,
    pub started_at_epoch_seconds: u64,
    pub stdout_log: Option<PathBuf>,
    pub stderr_log: Option<PathBuf>,
}

pub fn runtime_state_path() -> PathBuf { paths::state_dir().join("runtime-state.json") }

pub fn save_runtime_state(state: &PersistedRuntimeState) -> Result<(), StateError> {
    paths::ensure_dirs()?;
    let text = serde_json::to_string_pretty(state).map_err(StateError::Json)?;
    fs::write(runtime_state_path(), text)?;
    Ok(())
}

pub fn load_runtime_state() -> Result<Option<PersistedRuntimeState>, StateError> {
    let path = runtime_state_path();
    if !path.is_file() { return Ok(None); }
    let text = fs::read_to_string(path)?;
    let state = serde_json::from_str(&text).map_err(StateError::Json)?;
    Ok(Some(state))
}

pub fn clear_runtime_state() -> Result<(), StateError> {
    let path = runtime_state_path();
    if path.is_file() { fs::remove_file(path)?; }
    Ok(())
}

