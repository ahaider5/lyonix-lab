use crate::config::paths;
use crate::domain::{NetworkPolicy, UserConfiguration};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("state directory error: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuration parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("configuration serialization error: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("runtime state serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub mod runtime_state;

pub fn user_config_path() -> PathBuf { paths::config_dir().join("config.toml") }

pub fn load_user_configuration() -> Result<UserConfiguration, StateError> {
    let path = user_config_path();
    if !path.is_file() { return Ok(UserConfiguration::default()); }
    Ok(toml::from_str(&fs::read_to_string(path)?)?)
}

pub fn save_user_configuration(config: &UserConfiguration) -> Result<(), StateError> {
    paths::ensure_dirs()?;
    let text = toml::to_string_pretty(config)?;
    fs::write(user_config_path(), text)?;
    Ok(())
}

pub fn effective_model_roots(default_root: &Path) -> Result<Vec<PathBuf>, StateError> {
    let config = load_user_configuration()?;
    Ok(if config.model_roots.is_empty() { vec![default_root.to_path_buf()] } else { config.model_roots })
}

pub fn effective_network(config: &UserConfiguration) -> NetworkPolicy {
    config.network.clone().unwrap_or_default()
}
